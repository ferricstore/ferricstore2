//! Bounded operation-eight audit and consumed replay under an explicit opt-in.

use std::mem::size_of;

use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{RaftLogEntry, prepare_workflow_value_batch};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, WorkflowValueTransactionLimits,
    apply_workflow_value_batch_transaction,
};
use wf_types::identity::{GroupId, RecoveryId, TenantId};
use wf_types::version::CommittedTime;

use super::{CommittedRecoveryError, CommittedRecoveryLimits, replay, total};

impl CommittedRecoveryLimits {
    /// Selects additional publication ceilings for value-entry recovery.
    /// # Errors
    /// Refuses zero ceilings or overflow in the whole-call work upper bound.
    pub fn with_workflow_value_limits(
        mut self,
        rows: usize,
        bytes: usize,
        work: usize,
        temporary: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        if [rows, bytes, work, temporary].contains(&0) {
            return Err(CommittedRecoveryError::InvalidLimits);
        }
        self.work
            .0
            .checked_mul(work)
            .ok_or(CommittedRecoveryError::Limit)?;
        self.workflow_values = Some([rows, bytes, work, temporary]);
        Ok(self)
    }

    /// Reads the value profile; legacy construction leaves it absent.
    #[must_use]
    pub const fn workflow_value_limits(self) -> Option<WorkflowValueTransactionLimits> {
        match self.workflow_values {
            Some([rows, bytes, work, temporary]) => Some(WorkflowValueTransactionLimits {
                record_batch: self.normal,
                maximum_value_rows: rows,
                maximum_value_bytes: bytes,
                maximum_value_work: work,
                maximum_value_temporary_bytes: temporary,
            }),
            None => None,
        }
    }
}

pub(super) fn audit(
    body: &[u8],
    limits: &CommittedRecoveryLimits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, CommittedRecoveryError> {
    let entry = total(&[512, body.len()])?;
    let profile_header = size_of::<Option<WorkflowValueTransactionLimits>>();
    let live = total(&[base, entry, profile_header])?;
    limits.admit(&[live])?;
    let profile = limits
        .workflow_value_limits()
        .ok_or(CommittedRecoveryError::UnsupportedEntry)?;
    let available = limits
        .workspace
        .checked_sub(live)
        .ok_or(CommittedRecoveryError::Limit)?;
    let (decoded, _spent) = prepare_workflow_value_batch(
        body,
        profile.record_batch.codec,
        available,
        profile.maximum_value_work,
    )
    .map_err(CommittedRecoveryError::Codec)?;
    drop(decoded);
    Ok(EntryStamp::Stamped(stamp))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    fields: [&[u8]; 3],
    context: (&CommittedRecoveryLimits, usize),
) -> Result<ControlState, CommittedRecoveryError> {
    let (limits, base) = context;
    let entry_owner = total(&[512, entry.body().len()])?;
    let profile_header = size_of::<Option<WorkflowValueTransactionLimits>>();
    limits.admit(&[base, entry_owner, profile_header])?;
    let profile = limits
        .workflow_value_limits()
        .ok_or(CommittedRecoveryError::UnsupportedEntry)?;
    let scope_owner = total(&[512, fields[0].len(), fields[1].len(), fields[2].len()])?;
    limits.admit(&[
        base,
        profile_header,
        entry_owner,
        scope_owner,
        profile.record_batch.workspace_bytes,
    ])?;
    let scope = RequestScope::new(
        TenantId::new(replay::copy(fields[0])?)
            .map_err(|_| CommittedRecoveryError::Codec(wf_ports::log::CodecError::Malformed))?,
        GroupId::new(replay::copy(fields[1])?)
            .map_err(|_| CommittedRecoveryError::Codec(wf_ports::log::CodecError::Malformed))?,
        RecoveryId::new(replay::copy(fields[2])?)
            .map_err(|_| CommittedRecoveryError::Codec(wf_ports::log::CodecError::Malformed))?,
    );
    let outcome =
        apply_workflow_value_batch_transaction(state, &scope, entry.body(), entry.id(), profile)
            .map_err(CommittedRecoveryError::Value)?;
    let control = match outcome {
        RecordBatchTransactionOutcome::Retained { control, outcome } => {
            drop(outcome);
            control
        }
        RecordBatchTransactionOutcome::IntentConflict(control)
        | RecordBatchTransactionOutcome::Expired(control)
        | RecordBatchTransactionOutcome::LogReplay(control) => control,
    };
    drop(scope);
    Ok(control)
}
