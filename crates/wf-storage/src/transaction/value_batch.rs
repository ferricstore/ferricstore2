//! Consumed workflow value transaction boundary and explicit logical limits.

use std::fmt;
use std::mem::size_of;

use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{CodecError, FreshRecordBatch, RecordBatchApplyError, RecordBatchApplyPlan};
use wf_ports::state::{GroupApplyChanges, StateTransactionError};
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

pub(super) mod driver;
mod plan;
mod prepare;
mod step;
mod step_continue;
pub(crate) use plan::{ValuePublicationPlan, ValueStep};

/// Applies an explicitly stored-payload Step with complete durable companions.
/// # Errors
/// Refuses unsupported capabilities, invalid authority and operational failures.
pub fn apply_stored_step_continue_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    step_continue::apply(store, scope, encoded, log, limits, 11)
}

/// Applies an explicitly selected Step through the complete-history/value writer.
/// # Errors
/// Refuses unsupported format, invalid authority, budgets or unproved commit.
pub fn apply_step_continue_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    step_continue::apply(store, scope, encoded, log, limits, 10)
}

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

/// Applies an explicitly stored-byte command to opted-in durable authority.
///
/// # Errors
/// Refuses unsupported capability or unproved durable completion.
pub fn apply_workflow_stored_value_batch_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    driver::apply(store, scope, encoded, log, limits, 9)
}

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
    driver::apply(store, scope, encoded, log, limits, 8)
}

fn commit_complete(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    predecessor: ControlState,
    result: RecordBatchApplyPlan<'_>,
    (prepared, intent_version): (wf_domain::workflow::control::PreparedDigest, u16),
    mode: u8,
    mut charge: ValueCharge,
) -> Result<(ControlState, u8), WorkflowValueTransactionError> {
    let changes = GroupApplyChanges::new(scope, predecessor, result).map_err(storage_error)?;
    charge
        .temporary(size_of::<ValuePublicationPlan<'_>>())
        .map_err(storage_error)?;
    let mut publication = plan::empty((prepared, intent_version), mode, charge);
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
    charge: ValueCharge,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    apply_fresh_selected(
        tx,
        scope,
        fresh,
        predecessor,
        limits,
        (live, driver::ValueApplyProfile::Legacy),
        charge,
    )
}

fn apply_fresh_selected(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    fresh: FreshRecordBatch<'_>,
    predecessor: ControlState,
    limits: &WorkflowValueTransactionLimits,
    (live, profile): (usize, driver::ValueApplyProfile),
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
    let (result, ordering) = if matches!(fresh.decoded().request().intent.version(), 10 | 11) {
        (
            observations::decide(fresh, scope, view, limits.record_batch, live)
                .map_err(record_error)?,
            Vec::new(),
        )
    } else {
        observations::decide_values(fresh, scope, view, limits.record_batch, live)
            .map_err(record_error)?
    };
    let (result, release) = super::workflow_release::prepare(
        profile,
        &tx,
        scope,
        result,
        limits.record_batch.codec.admission(),
    )?;
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
    if let Some(release) = &release {
        tx.commit_workflow_release_values(&changes, &mut publication, release)
            .map_err(storage_error)?;
    } else {
        tx.commit_values(&changes, &mut publication)
            .map_err(storage_error)?;
    }
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
    if matches!(fresh.decoded().request().intent.version(), 10 | 11) {
        return step_continue::prepare_fresh(tx, scope, fresh, limits, schema, charge);
    }
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
