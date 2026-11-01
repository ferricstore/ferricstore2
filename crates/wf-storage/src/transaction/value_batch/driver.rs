//! Explicit-version preparation and one coherent retained/fresh transaction.

use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::log::{
    DecodedRequest, RecordBatchInspection, inspect_record_batch_point,
    prepare_workflow_stored_value_batch, prepare_workflow_value_batch,
};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_types::version::LogIdentity;

use super::{
    RecordBatchTransactionOutcome, RedbStateStore, ValueCharge, WorkflowValueTransactionError,
    WorkflowValueTransactionLimits, apply_fresh_selected, codec_error, commit_complete,
    record_batch, record_error, storage_error, total,
};

#[derive(Clone, Copy)]
pub(in crate::transaction) enum ValueApplyProfile {
    Legacy,
    WorkflowRelease,
}

pub(super) fn apply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
    version: u16,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    apply_selected(
        store,
        scope,
        encoded,
        log,
        limits,
        (version, ValueApplyProfile::Legacy),
    )
}

pub(in crate::transaction) fn apply_release(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    apply_selected(
        store,
        scope,
        encoded,
        log,
        limits,
        (9, ValueApplyProfile::WorkflowRelease),
    )
}

fn apply_selected(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
    (version, profile): (u16, ValueApplyProfile),
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    let mut charge = ValueCharge::new(
        limits.maximum_value_rows,
        limits.maximum_value_bytes,
        limits.maximum_value_work,
        limits.maximum_value_temporary_bytes,
    )
    .map_err(storage_error)?;
    let (decoded, live) = decode(encoded, &limits, &mut charge, version)?;
    if decoded.request().scope != *scope || decoded.request().intent.version() != version {
        return Err(WorkflowValueTransactionError::InvalidInput);
    }
    let mode = decoded
        .workflow_value_metadata()
        .ok_or(WorkflowValueTransactionError::UnsupportedProfile)?
        .profile;
    if matches!(profile, ValueApplyProfile::WorkflowRelease) && mode == 7 {
        return Err(WorkflowValueTransactionError::UnsupportedProfile);
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
            WorkflowValueTransactionError::UnsupportedProfile
        } else {
            storage_error(error)
        }
    })?;
    if matches!(version, 9 | 22) && manifest.alias_format != 2 {
        return Err(WorkflowValueTransactionError::UnsupportedProfile);
    }
    if version == 22 && manifest.binding_format != 2 {
        return Err(WorkflowValueTransactionError::UnsupportedProfile);
    }
    if matches!(profile, ValueApplyProfile::WorkflowRelease) {
        tx.require_workflow_release().map_err(storage_error)?;
    }
    charge.helper().map_err(storage_error)?;
    let predecessor = tx.control().map_err(storage_error)?;
    let retained_nonempty = tx
        .retained_nonempty(scope.tenant())
        .map_err(storage_error)?;
    let original = tx
        .outcome(scope.tenant(), &decoded.request().request)
        .map_err(storage_error)?;
    let inspection = inspect_record_batch_point(
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
            let (control, kind) = commit_complete(
                tx,
                scope,
                predecessor,
                result,
                (prepared, version),
                mode,
                charge,
            )?;
            record_batch::complete_reply(control, kind, original).map_err(record_error)
        }
        RecordBatchInspection::Fresh(fresh) => {
            drop(original);
            apply_fresh_selected(
                tx,
                scope,
                fresh,
                predecessor,
                &limits,
                (live, profile, manifest.binding_format),
                charge,
            )
        }
    }
}

fn decode<'input>(
    encoded: &'input [u8],
    limits: &WorkflowValueTransactionLimits,
    charge: &mut ValueCharge,
    version: u16,
) -> Result<(DecodedRequest<'input>, usize), WorkflowValueTransactionError> {
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
    if version == 22 {
        let (decoded, spent) = wf_ports::log::prepare_workflow_metadata_batch(
            encoded,
            wf_ports::log::MetadataBatchLimits {
                codec: limits.record_batch.codec,
                maximum_owned_bytes: available,
                maximum_work_units: charge.remaining_work(),
            },
        )
        .map_err(codec_error)?;
        charge.work(spent).map_err(storage_error)?;
        return Ok((decoded, live));
    }
    let prepare = match version {
        8 => prepare_workflow_value_batch,
        9 => prepare_workflow_stored_value_batch,
        _ => return Err(WorkflowValueTransactionError::UnsupportedProfile),
    };
    let (decoded, spent) = prepare(
        encoded,
        limits.record_batch.codec,
        available,
        charge.remaining_work(),
    )
    .map_err(codec_error)?;
    charge.work(spent).map_err(storage_error)?;
    Ok((decoded, live))
}
