//! Plain Step joins the actual retained gate and same consumed value/core writer.

use std::mem::size_of;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::consensus::{WorkflowValueItemInput, WorkflowValueTargetGuard};
use wf_ports::log::{
    FreshRecordBatch, RecordBatchInspection, inspect_step_continue_point, prepare_step_continue,
};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_types::version::LogIdentity;

use super::{
    RecordBatchTransactionOutcome, RedbStateStore, ValueCharge,
    WorkflowValueTransactionError as Error, WorkflowValueTransactionLimits, apply_fresh, binding,
    commit_complete, observations, prepare::ValueInputs, record_batch, record_error, storage_error,
    total,
};
use crate::state::value::ValueBindingData;
use crate::transaction::load::LoadedRecordBatch;

pub(super) fn apply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, Error> {
    let mut charge = ValueCharge::new(
        limits.maximum_value_rows,
        limits.maximum_value_bytes,
        limits.maximum_value_work,
        limits.maximum_value_temporary_bytes,
    )
    .map_err(storage_error)?;
    let live = total(
        record_batch::live_bytes(limits.record_batch).map_err(record_error)?,
        [limits.maximum_value_temporary_bytes],
    )
    .map_err(storage_error)?;
    let available = limits
        .record_batch
        .workspace_bytes
        .checked_sub(live)
        .ok_or_else(|| storage_error(StateTransactionError::Limit))?;
    charge
        .work(
            total(
                4096,
                [encoded
                    .len()
                    .checked_mul(16)
                    .ok_or_else(|| storage_error(StateTransactionError::Limit))?],
            )
            .map_err(storage_error)?,
        )
        .map_err(storage_error)?;
    let decoded = prepare_step_continue(encoded, limits.record_batch.codec, available)
        .map_err(|error| record_error(error.into()))?;
    if decoded.request().scope != *scope {
        return Err(Error::InvalidInput);
    }
    let prepared = decoded.digest();
    let tx = store
        .begin_apply(
            scope.group(),
            scope.recovery(),
            limits.record_batch.codec.admission(),
            limits.record_batch.maximum_read_steps,
            limits.record_batch.maximum_read_bytes,
            limits.record_batch.maximum_adapter_bytes,
        )
        .map_err(storage_error)?;
    let manifest = tx.value_manifest().map_err(|error| {
        if error == StateTransactionError::UnsupportedLayout {
            Error::UnsupportedProfile
        } else {
            storage_error(error)
        }
    })?;
    if manifest.layout != 8 {
        return Err(Error::UnsupportedProfile);
    }
    charge.helper().map_err(storage_error)?;
    let predecessor = tx.control().map_err(storage_error)?;
    let retained_nonempty = tx
        .retained_nonempty(scope.tenant())
        .map_err(storage_error)?;
    let original = tx
        .outcome(scope.tenant(), &decoded.request().request)
        .map_err(storage_error)?;
    let inspection = inspect_step_continue_point(
        decoded,
        log,
        PointGateView {
            scope,
            control: predecessor,
            outcome: original.as_ref(),
            retained_nonempty,
        },
        limits.record_batch.codec,
        limits.record_batch.workspace_bytes,
    )
    .map_err(|error| record_error(error.into()))?;
    match inspection {
        RecordBatchInspection::Complete(result) => {
            let (control, kind) =
                commit_complete(tx, scope, predecessor, result, (prepared, 10), 10, charge)?;
            record_batch::complete_reply(control, kind, original).map_err(record_error)
        }
        RecordBatchInspection::Fresh(fresh) => {
            drop(original);
            apply_fresh(tx, scope, fresh, predecessor, &limits, live, charge)
        }
    }
}

pub(super) fn prepare_fresh(
    tx: &super::RedbStateTransaction<'_>,
    scope: &RequestScope,
    fresh: &FreshRecordBatch<'_>,
    limits: &WorkflowValueTransactionLimits,
    schema: &mut PolicySchemaCharge,
    charge: &mut ValueCharge,
) -> Result<
    (
        LoadedRecordBatch,
        Vec<Option<ValueBindingData>>,
        ValueInputs,
    ),
    Error,
> {
    let mut loaded =
        LoadedRecordBatch::load_points_only(tx, scope, fresh.decoded(), limits.record_batch)
            .map_err(record_error)?;
    let bindings = binding::load(tx, scope, &loaded.queries, &loaded.records, charge)
        .map_err(storage_error)?;
    if bindings.iter().flatten().any(|binding| {
        binding
            .lifecycle
            .terminal_deadline
            .is_some_and(|end| end <= fresh.control().execution_time())
    }) {
        return Err(Error::UnsupportedProfile);
    }
    loaded
        .load_policies(tx, scope, limits.record_batch, schema)
        .map_err(record_error)?;
    observations::load_indexes(
        &mut loaded,
        tx,
        scope,
        limits.record_batch.maximum_staging_bytes,
    )
    .map_err(record_error)?;
    let fixed = total(
        size_of::<ValueInputs>(),
        [
            size_of::<PolicySchemaCharge>(),
            size_of::<ValueCharge>(),
            size_of::<Vec<Option<ValueBindingData>>>(),
            bindings
                .len()
                .checked_mul(size_of::<Option<ValueBindingData>>())
                .ok_or_else(|| storage_error(StateTransactionError::Limit))?,
        ],
    )
    .map_err(storage_error)?;
    let mut inputs = charge.reserve(1, fixed).map_err(storage_error)?;
    inputs.push(WorkflowValueItemInput {
        source_index: 0,
        policy_index: 0,
        target: WorkflowValueTargetGuard::Unspecified,
        birth: None,
        terminal_ttl_ms: None,
        requested_at: None,
    });
    let selected = ValueInputs {
        prepared: fresh.decoded().digest(),
        mode: 10,
        intent_version: 10,
        defaults: tx.value_manifest().map_err(storage_error)?.defaults,
        inputs,
        policies: Vec::new(),
    };
    Ok((loaded, bindings, selected))
}
