//! Bounded binary/stored value audit and consumed replay under explicit opt-ins.

use std::mem::size_of;
use wf_domain::claim::apply::ClaimWorkLimits;

use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{
    RaftLogEntry, prepare_claim_phase, prepare_step_continue, prepare_stored_step_continue,
    prepare_workflow_stored_value_batch, prepare_workflow_value_batch,
};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionOutcome, RecordBatchTransactionOutcome, WorkflowValueTransactionError,
    WorkflowValueTransactionLimits, apply_step_continue_transaction,
    apply_stored_step_continue_transaction, apply_workflow_stored_value_batch_transaction,
    apply_workflow_value_batch_transaction, apply_workflow_value_claim_transaction,
};
use wf_types::identity::{GroupId, RecoveryId, TenantId};
use wf_types::version::CommittedTime;

use super::{CommittedRecoveryError, CommittedRecoveryLimits, replay, total};

#[derive(Clone, Copy)]
pub(super) enum StepRecoveryProfile {
    Disabled,
    Enabled,
    Stored,
    Both,
}

impl StepRecoveryProfile {
    pub(super) const fn new(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }

    pub(super) const fn enabled(self) -> bool {
        matches!(self, Self::Enabled | Self::Both)
    }

    pub(super) const fn stored(self) -> bool {
        matches!(self, Self::Stored | Self::Both)
    }

    const fn with_plain(self) -> Self {
        if self.stored() {
            Self::Both
        } else {
            Self::Enabled
        }
    }

    const fn with_stored(self) -> Self {
        if self.enabled() {
            Self::Both
        } else {
            Self::Stored
        }
    }
}

impl CommittedRecoveryLimits {
    /// Selects stored-payload Step recovery and its value ceilings.
    /// # Errors
    /// Refuses unsupported selections, invalid ceilings or overflow.
    pub fn with_stored_step_continue_limits(
        self,
        rows: usize,
        bytes: usize,
        work: usize,
        temporary: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        let mut selected = self.with_workflow_value_limits(rows, bytes, work, temporary)?;
        selected.step_continue = selected.step_continue.with_stored();
        Ok(selected)
    }

    /// Reads shared ceilings only when stored Step recovery was selected.
    #[must_use]
    pub const fn stored_step_continue_limits(self) -> Option<WorkflowValueTransactionLimits> {
        if self.step_continue.stored() {
            self.workflow_value_limits()
        } else {
            None
        }
    }

    /// Selects named Claim recovery with its actual query and value ceilings.
    /// # Errors
    /// Refuses unsupported selection, invalid ceilings or whole-call overflow.
    pub fn with_workflow_value_claim_limits(
        self,
        claim_work: ClaimWorkLimits,
        rows: usize,
        bytes: usize,
        work: usize,
        temporary: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        let mut selected = self.with_workflow_value_limits(rows, bytes, work, temporary)?;
        selected.value_claim_work = Some(claim_work);
        Ok(selected)
    }

    /// Reads only explicitly selected named Claim work and shared value limits.
    #[must_use]
    pub const fn workflow_value_claim_limits(
        self,
    ) -> Option<(ClaimWorkLimits, WorkflowValueTransactionLimits)> {
        match (self.value_claim_work, self.workflow_value_limits()) {
            (Some(work), Some(profile)) => Some((work, profile)),
            _ => None,
        }
    }

    /// Selects explicit Step startup recovery and its shared value ceilings.
    /// # Errors
    /// Refuses unsupported selection, zero ceilings or whole-call work overflow.
    pub fn with_step_continue_limits(
        self,
        rows: usize,
        bytes: usize,
        work: usize,
        temporary: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        let mut selected = self.with_workflow_value_limits(rows, bytes, work, temporary)?;
        selected.step_continue = selected.step_continue.with_plain();
        Ok(selected)
    }

    /// Reads the shared ceilings only when Step recovery was explicitly selected.
    #[must_use]
    pub const fn step_continue_limits(self) -> Option<WorkflowValueTransactionLimits> {
        if self.step_continue.enabled() {
            self.workflow_value_limits()
        } else {
            None
        }
    }

    /// Selects explicit stored-value startup recovery and its shared ceilings.
    /// # Errors
    /// Refuses zero ceilings or overflow in the whole-call work upper bound.
    pub fn with_workflow_stored_value_limits(
        self,
        rows: usize,
        bytes: usize,
        work: usize,
        temporary: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        let mut selected = self.with_workflow_value_limits(rows, bytes, work, temporary)?;
        selected.stored_workflow_values = true;
        Ok(selected)
    }

    /// Reads the shared ceilings only when stored recovery was explicitly selected.
    #[must_use]
    pub const fn workflow_stored_value_limits(self) -> Option<WorkflowValueTransactionLimits> {
        if self.stored_workflow_values {
            self.workflow_value_limits()
        } else {
            None
        }
    }

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
    version: u16,
) -> Result<EntryStamp, CommittedRecoveryError> {
    let entry = total(&[512, body.len()])?;
    let profile_header = header_size(version)?;
    let live = total(&[base, entry, profile_header])?;
    limits.admit(&[live])?;
    let profile = profile(limits, version)?;
    let available = limits
        .workspace
        .checked_sub(live)
        .ok_or(CommittedRecoveryError::Limit)?;
    if matches!(version, 3 | 10 | 11) {
        let work = total(&[
            65_536,
            4096,
            body.len()
                .checked_mul(16)
                .ok_or(CommittedRecoveryError::Limit)?,
        ])?;
        if work > profile.maximum_value_work {
            return Err(CommittedRecoveryError::Limit);
        }
        let prepare = match version {
            3 => prepare_claim_phase,
            11 => prepare_stored_step_continue,
            _ => prepare_step_continue,
        };
        let decoded = prepare(body, profile.record_batch.codec, available).map_err(|error| {
            if version == 3 {
                CommittedRecoveryError::Claim(error.into())
            } else {
                CommittedRecoveryError::Value(WorkflowValueTransactionError::Record(error.into()))
            }
        })?;
        drop(decoded);
        return Ok(EntryStamp::Stamped(stamp));
    }
    let prepare = if version == 9 {
        prepare_workflow_stored_value_batch
    } else {
        prepare_workflow_value_batch
    };
    let (decoded, _spent) = prepare(
        body,
        profile.record_batch.codec,
        available,
        profile.maximum_value_work,
    )
    .map_err(CommittedRecoveryError::Codec)?;
    if super::workflow_release::enabled(limits)
        && decoded
            .workflow_value_metadata()
            .is_some_and(|metadata| metadata.profile == 7)
    {
        return Err(CommittedRecoveryError::UnsupportedEntry);
    }
    drop(decoded);
    Ok(EntryStamp::Stamped(stamp))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    fields: [&[u8]; 3],
    context: (&CommittedRecoveryLimits, usize, u16),
) -> Result<ControlState, CommittedRecoveryError> {
    let (limits, base, version) = context;
    let entry_owner = total(&[512, entry.body().len()])?;
    let profile_header = header_size(version)?;
    limits.admit(&[base, entry_owner, profile_header])?;
    let profile = profile(limits, version)?;
    let scope_owner = total(&[512, fields[0].len(), fields[1].len(), fields[2].len()])?;
    let conversion = total(&[
        fields[0].len().max(fields[1].len()).max(fields[2].len()),
        size_of::<Vec<u8>>(),
    ])?;
    limits.admit(&[
        base,
        profile_header,
        entry_owner,
        scope_owner,
        profile.record_batch.workspace_bytes.max(conversion),
    ])?;
    let scope = RequestScope::new(
        TenantId::new(replay::copy(fields[0])?)
            .map_err(|_| CommittedRecoveryError::Codec(wf_ports::log::CodecError::Malformed))?,
        GroupId::new(replay::copy(fields[1])?)
            .map_err(|_| CommittedRecoveryError::Codec(wf_ports::log::CodecError::Malformed))?,
        RecoveryId::new(replay::copy(fields[2])?)
            .map_err(|_| CommittedRecoveryError::Codec(wf_ports::log::CodecError::Malformed))?,
    );
    if version == 3 {
        let (work, _) = limits
            .workflow_value_claim_limits()
            .ok_or(CommittedRecoveryError::UnsupportedEntry)?;
        let outcome = apply_workflow_value_claim_transaction(
            state,
            &scope,
            entry.body(),
            entry.id(),
            profile,
            work,
        )
        .map_err(CommittedRecoveryError::Claim)?;
        let control = match outcome {
            ClaimTransactionOutcome::Retained { control, outcome } => {
                drop(outcome);
                control
            }
            ClaimTransactionOutcome::IntentConflict(control)
            | ClaimTransactionOutcome::Expired(control)
            | ClaimTransactionOutcome::LogReplay(control) => control,
        };
        drop(scope);
        return Ok(control);
    }
    let driver = if version == 9 && super::workflow_release::enabled(limits) {
        wf_storage::transaction::apply_workflow_release_transaction
    } else if version == 9 {
        apply_workflow_stored_value_batch_transaction
    } else if version == 11 {
        apply_stored_step_continue_transaction
    } else if version == 10 {
        apply_step_continue_transaction
    } else {
        apply_workflow_value_batch_transaction
    };
    let outcome = driver(state, &scope, entry.body(), entry.id(), profile)
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

fn profile(
    limits: &CommittedRecoveryLimits,
    version: u16,
) -> Result<WorkflowValueTransactionLimits, CommittedRecoveryError> {
    let selected = match version {
        3 => limits
            .workflow_value_claim_limits()
            .map(|(_, profile)| profile),
        8 => limits.workflow_value_limits(),
        9 => limits.workflow_stored_value_limits(),
        10 => limits.step_continue_limits(),
        11 => limits.stored_step_continue_limits(),
        _ => None,
    };
    selected.ok_or(CommittedRecoveryError::UnsupportedEntry)
}

fn header_size(version: u16) -> Result<usize, CommittedRecoveryError> {
    if version == 3 {
        Ok(size_of::<
            Option<(ClaimWorkLimits, WorkflowValueTransactionLimits)>,
        >())
    } else if version == 9 {
        total(&[
            size_of::<Option<WorkflowValueTransactionLimits>>(),
            size_of::<bool>(),
        ])
    } else if matches!(version, 10 | 11) {
        total(&[
            size_of::<Option<WorkflowValueTransactionLimits>>(),
            size_of::<StepRecoveryProfile>(),
        ])
    } else {
        Ok(size_of::<Option<WorkflowValueTransactionLimits>>())
    }
}
