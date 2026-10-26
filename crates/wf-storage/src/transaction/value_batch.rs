//! Consumed workflow value transaction boundary and explicit logical limits.

use std::fmt;
use std::mem::size_of;

use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::log::{
    CodecError, DecodedRequest, FreshRecordBatch, RecordBatchApplyError, RecordBatchApplyPlan,
    RecordBatchInspection, inspect_record_batch_point, prepare_workflow_value_batch,
};
use wf_ports::state::{
    GroupApplyChanges, GroupApplyStore, GroupApplyTransaction, StateTransactionError,
};
use wf_types::version::LogIdentity;

use super::load::LoadedRecordBatch;
use super::{observations, record_batch};
use crate::state::redb::values::{
    binding,
    budget::{ValueCharge, total},
};
use crate::state::{RedbStateStore, RedbStateTransaction};

use super::{
    RecordBatchTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
};

mod plan;
mod prepare;
mod step;
pub(crate) use plan::{ValuePublicationPlan, ValueStep};

/// Additional publication ceilings, carried with the existing driver profile.
#[derive(Clone, Copy)]
pub struct WorkflowValueTransactionLimits {
    /// Existing decoder, policy, staging and adapter admission profile.
    pub record_batch: RecordBatchTransactionLimits,
    /// Maximum admitted value-related rows.
    pub maximum_value_rows: usize,
    /// Maximum cumulative admitted value bytes.
    pub maximum_value_bytes: usize,
    /// Maximum cumulative charged value work.
    pub maximum_value_work: usize,
    /// Maximum simultaneous logical temporary value ownership.
    pub maximum_value_temporary_bytes: usize,
}

/// Operational refusal; never retained as a fabricated business outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowValueTransactionError {
    /// Existing record driver refusal.
    Record(RecordBatchTransactionError),
    /// Prepared inputs violate the selected semantic profile.
    InvalidInput,
    /// Required captured policy reference is absent.
    PolicyReferenceRequired,
    /// The selected policy generation has not reached this authority.
    PolicyGenerationNotApplied,
    /// Current authority has advanced beyond the prepared generation.
    StalePolicyGeneration,
    /// Equal generations have different retained policy backing.
    ConflictingPolicyGeneration,
    /// The prepared target no longer identifies the current birth.
    StalePolicyTarget,
    /// This operation or persistent profile is not yet supported.
    UnsupportedProfile,
}

impl fmt::Display for WorkflowValueTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Record(error) => fmt::Display::fmt(error, formatter),
            Self::InvalidInput => formatter.write_str("invalid workflow value input"),
            Self::PolicyReferenceRequired => formatter.write_str("policy reference required"),
            Self::PolicyGenerationNotApplied => {
                formatter.write_str("policy generation not applied")
            }
            Self::StalePolicyGeneration => formatter.write_str("stale policy generation"),
            Self::ConflictingPolicyGeneration => {
                formatter.write_str("conflicting policy generation")
            }
            Self::StalePolicyTarget => formatter.write_str("stale policy target"),
            Self::UnsupportedProfile => formatter.write_str("unsupported workflow value profile"),
        }
    }
}

impl std::error::Error for WorkflowValueTransactionError {}

/// Applies one prepared value command through the existing consumed driver.
///
/// # Errors
/// Refuses unsupported authority, stale preparation, resource limits or unproved
/// durable completion. No owned reply escapes before the consumed commit.
pub fn apply_workflow_value_batch_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    let mut charge = ValueCharge::new(
        limits.maximum_value_rows,
        limits.maximum_value_bytes,
        limits.maximum_value_work,
        limits.maximum_value_temporary_bytes,
    )
    .map_err(storage_error)?;
    let (decoded, live) = decode(encoded, &limits, &mut charge)?;
    if decoded.request().scope != *scope || decoded.request().intent.version() != 8 {
        return Err(WorkflowValueTransactionError::InvalidInput);
    }
    let mode = decoded
        .workflow_value_metadata()
        .ok_or(WorkflowValueTransactionError::UnsupportedProfile)?
        .profile;
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
    tx.value_manifest().map_err(|error| {
        if error == StateTransactionError::UnsupportedLayout {
            WorkflowValueTransactionError::UnsupportedProfile
        } else {
            storage_error(error)
        }
    })?;
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
            let (control, kind) =
                commit_complete(tx, scope, predecessor, result, prepared, mode, charge)?;
            record_batch::complete_reply(control, kind, original).map_err(record_error)
        }
        RecordBatchInspection::Fresh(fresh) => {
            drop(original);
            apply_fresh(tx, scope, fresh, predecessor, &limits, live, charge)
        }
    }
}

fn decode<'input>(
    encoded: &'input [u8],
    limits: &WorkflowValueTransactionLimits,
    charge: &mut ValueCharge,
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
    let (decoded, spent) = prepare_workflow_value_batch(
        encoded,
        limits.record_batch.codec,
        available,
        charge.remaining_work(),
    )
    .map_err(codec_error)?;
    charge.work(spent).map_err(storage_error)?;
    Ok((decoded, live))
}

fn commit_complete(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    predecessor: ControlState,
    result: RecordBatchApplyPlan<'_>,
    prepared: wf_domain::workflow::control::PreparedDigest,
    mode: u8,
    mut charge: ValueCharge,
) -> Result<(ControlState, u8), WorkflowValueTransactionError> {
    let changes = GroupApplyChanges::new(scope, predecessor, result).map_err(storage_error)?;
    charge
        .temporary(size_of::<ValuePublicationPlan<'_>>())
        .map_err(storage_error)?;
    let mut publication = plan::empty(prepared, mode, charge);
    tx.commit_values(&changes, &mut publication)
        .map_err(storage_error)?;
    record_batch::complete_kind(changes.into_plan()).map_err(record_error)
}

fn apply_fresh(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    fresh: FreshRecordBatch<'_>,
    predecessor: ControlState,
    limits: &WorkflowValueTransactionLimits,
    live: usize,
    mut charge: ValueCharge,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    charge.helper().map_err(storage_error)?;
    charge
        .temporary(
            total(size_of::<ValueCharge>(), [size_of::<PolicySchemaCharge>()])
                .map_err(storage_error)?,
        )
        .map_err(storage_error)?;
    let mut schema = PolicySchemaCharge::new(limits.record_batch.schema);
    let (mut loaded, bindings, inputs) =
        prepare_fresh(&tx, scope, &fresh, limits, &mut schema, &mut charge)?;
    let LoadedRecordBatch {
        queries,
        records,
        placements,
        policy_types,
        policies,
        projections,
        parallel,
        members,
        heads,
        staging_used,
    } = &mut loaded;
    let view = observations::RecordBatchObservationView {
        queries,
        records,
        placements,
        policy_types,
        projections,
        parallel,
        members,
        heads,
        staging_used,
    };
    admit_decision_work(policy_types, inputs.inputs.len(), &mut charge)?;
    let (result, ordering) =
        observations::decide_values(fresh, scope, view, limits.record_batch, live)
            .map_err(record_error)?;
    let changes = GroupApplyChanges::new(scope, predecessor, result).map_err(storage_error)?;
    let mut publication = plan::build(
        &changes,
        &inputs,
        (queries, &bindings),
        &mut (policies, &mut schema),
        &ordering,
        limits,
        charge,
    )?;
    tx.commit_values(&changes, &mut publication)
        .map_err(storage_error)?;
    drop(publication);
    drop((ordering, inputs, bindings, schema));
    record_batch::fresh_reply(changes.into_plan()).map_err(record_error)
}

fn prepare_fresh(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    fresh: &FreshRecordBatch<'_>,
    limits: &WorkflowValueTransactionLimits,
    schema: &mut PolicySchemaCharge,
    charge: &mut ValueCharge,
) -> Result<
    (
        LoadedRecordBatch,
        Vec<Option<crate::state::value::ValueBindingData>>,
        prepare::ValueInputs,
    ),
    WorkflowValueTransactionError,
> {
    charge.helper().map_err(storage_error)?;
    let mut loaded =
        LoadedRecordBatch::load_points_only(tx, scope, fresh.decoded(), limits.record_batch)
            .map_err(record_error)?;
    let bindings = binding::load(tx, scope, &loaded.queries, &loaded.records, charge)
        .map_err(storage_error)?;
    prepare::targets(
        fresh.decoded(),
        &loaded,
        &bindings,
        fresh.control().execution_time(),
        charge,
    )?;
    prepare::types(
        fresh.decoded(),
        &mut loaded,
        charge,
        limits.record_batch.maximum_staging_bytes,
    )?;
    loaded
        .load_policies(tx, scope, limits.record_batch, schema)
        .map_err(record_error)?;
    let defaults = tx.value_manifest().map_err(storage_error)?.defaults;
    let inputs = prepare::bind(fresh.decoded(), &loaded, defaults, limits, schema, charge)?;
    admit_decision_work(&loaded.policy_types, inputs.inputs.len(), charge)?;
    observations::load_indexes(
        &mut loaded,
        tx,
        scope,
        limits.record_batch.maximum_staging_bytes,
    )
    .map_err(record_error)?;
    Ok((loaded, bindings, inputs))
}

fn admit_decision_work(
    types: &[Vec<u8>],
    items: usize,
    charge: &mut ValueCharge,
) -> Result<(), WorkflowValueTransactionError> {
    use crate::state::redb::values::budget::product;
    let count = types.len();
    charge
        .work(product(count, 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    let bytes = types
        .iter()
        .try_fold(0, |sum, kind| total(sum, [kind.len()]))
        .map_err(storage_error)?;
    let pairs = product(items, count).map_err(storage_error)?;
    let comparisons =
        total(product(count, count).map_err(storage_error)?, [pairs]).map_err(storage_error)?;
    let byte_factor = total(count, [items, pairs]).map_err(storage_error)?;
    let work = total(
        65_536,
        [
            product(items, 4_096).map_err(storage_error)?,
            product(count, 4_096).map_err(storage_error)?,
            product(comparisons, 128).map_err(storage_error)?,
            product(product(byte_factor, bytes).map_err(storage_error)?, 4)
                .map_err(storage_error)?,
        ],
    )
    .map_err(storage_error)?;
    charge.work(work).map_err(storage_error)
}

pub(super) fn storage_error(error: StateTransactionError) -> WorkflowValueTransactionError {
    record_error(error.into())
}

pub(super) fn record_error(error: RecordBatchTransactionError) -> WorkflowValueTransactionError {
    WorkflowValueTransactionError::Record(error)
}

fn codec_error(error: CodecError) -> WorkflowValueTransactionError {
    match error {
        CodecError::Allocation => storage_error(StateTransactionError::Allocation),
        CodecError::ByteLimit
        | CodecError::ItemLimit
        | CodecError::ValueLimit
        | CodecError::LengthOverflow
        | CodecError::OwnedLimit
        | CodecError::WorkLimit => storage_error(StateTransactionError::Limit),
        _ => record_error(RecordBatchApplyError::Encoding(error).into()),
    }
}
