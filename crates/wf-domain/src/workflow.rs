//! A pure reducer for one authoritative record, with explicit committed inputs.

use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use crate::history::{EventKind, LatestEvent};
use crate::lease::Lease;
use crate::policy::InstalledRetryPolicy;
pub use crate::value::DomainError;

mod command;
mod create_claim;
mod lifecycle;
mod plan;
mod retry_transition;
mod validation;

pub mod active;
pub mod admission;
pub mod batch;
pub mod caller;
pub mod control;
pub mod indexed;
pub mod request;
pub mod restoration;

pub use command::{
    Cancel, CommittedCommand, Create, Operation, StartAndClaim, StepContinue, Terminal, Transition,
};
pub use plan::{Outcome, RecordWrite, TransitionPlan};

use create_claim::{claim_record, create_record, start_and_claim_record};
use lifecycle::{
    cancel_record, extend_record, step_continue_record, terminal_record, transition_record,
};
use retry_transition::{policy_retry_record, retry_record};
use validation::{require_record, validate_run_at};

#[cfg(test)]
mod tests;

/// Source numeric ceiling for admitted caller millisecond inputs.
pub const MAX_EXACT_MILLIS: u64 = 9_007_199_254_740_991;

/// Lifecycle category, separate from an arbitrary binary logical handler state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowStatus {
    /// Unleased logical state with a scheduled deadline.
    Scheduled,
    /// Unleased nonterminal state without a scheduled deadline.
    Unscheduled,
    /// Physical running label; restoration may preserve absent ownership.
    Running,
    /// Successful terminal state.
    Completed,
    /// Failed terminal state.
    Failed,
    /// Cancelled terminal state.
    Cancelled,
}

/// Immutable authoritative read model; only the reducer constructs mutations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowRecord {
    id: RecordId,
    workflow_type: Vec<u8>,
    state: Vec<u8>,
    version: u64,
    fence: Fence,
    attempts: u64,
    created_at: TimestampMillis,
    updated_at: TimestampMillis,
    next_run_at: Option<TimestampMillis>,
    run_state: Option<Vec<u8>>,
    lease: Option<Lease>,
    payload: Option<Vec<u8>>,
    result: Option<Vec<u8>>,
    error: Option<Vec<u8>>,
    latest_event: LatestEvent,
}

impl WorkflowRecord {
    /// Borrows the stable record identity.
    #[must_use]
    pub const fn id(&self) -> &RecordId {
        &self.id
    }
    /// Borrows the binary workflow type.
    #[must_use]
    pub fn workflow_type(&self) -> &[u8] {
        &self.workflow_type
    }
    /// Borrows the lifecycle state, including the source `running` label.
    #[must_use]
    pub fn state(&self) -> &[u8] {
        &self.state
    }
    /// Reads the monotonic record version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    /// Reads the current fencing generation, retained after lease release.
    #[must_use]
    pub const fn fence(&self) -> Fence {
        self.fence
    }
    /// Reads retry attempts; claim does not increment this counter.
    #[must_use]
    pub const fn attempts(&self) -> u64 {
        self.attempts
    }
    /// Reads original creation time.
    #[must_use]
    pub const fn created_at(&self) -> TimestampMillis {
        self.created_at
    }
    /// Reads the last mutation's committed time.
    #[must_use]
    pub const fn updated_at(&self) -> TimestampMillis {
        self.updated_at
    }
    /// Reads a scheduled run time or current lease deadline.
    #[must_use]
    pub const fn next_run_at(&self) -> Option<TimestampMillis> {
        self.next_run_at
    }
    /// Borrows the source logical state retained separately from `running`.
    #[must_use]
    pub fn run_state(&self) -> Option<&[u8]> {
        self.run_state.as_deref()
    }
    /// Borrows the current lease, if any.
    #[must_use]
    pub const fn lease(&self) -> Option<&Lease> {
        self.lease.as_ref()
    }
    /// Reads source-compatible zero when no lease exists.
    #[must_use]
    pub fn lease_deadline(&self) -> TimestampMillis {
        self.lease
            .as_ref()
            .map_or(TimestampMillis::new(0), |lease| lease.deadline)
    }
    /// Borrows optional inline payload; empty bytes remain present.
    #[must_use]
    pub fn payload(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }
    /// Borrows optional successful result bytes.
    #[must_use]
    pub fn result(&self) -> Option<&[u8]> {
        self.result.as_deref()
    }
    /// Borrows optional error bytes.
    #[must_use]
    pub fn error(&self) -> Option<&[u8]> {
        self.error.as_deref()
    }
    /// Reads the exact latest event witness without inferring it from state.
    #[must_use]
    pub const fn latest_event_witness(&self) -> (EventKind, u64, Fence) {
        (
            self.latest_event.kind,
            self.latest_event.version,
            self.latest_event.fence,
        )
    }
    /// Derives lifecycle category without consulting time.
    #[must_use]
    pub fn status(&self) -> WorkflowStatus {
        match self.state.as_slice() {
            b"running" => WorkflowStatus::Running,
            b"completed" => WorkflowStatus::Completed,
            b"failed" => WorkflowStatus::Failed,
            b"cancelled" => WorkflowStatus::Cancelled,
            _ if self.next_run_at.is_some() => WorkflowStatus::Scheduled,
            _ => WorkflowStatus::Unscheduled,
        }
    }
}

/// Decides a command against one coherent authoritative record.
///
/// The caller must serialize decisions and atomically persist both described
/// writes before exposing a durable success. Represented binary/numeric fields
/// and ownership guards are revalidated with supported legacy errors. Routing,
/// composed-key/payload-size admission and storage are outside this reduced
/// command surface. Policy retries without an installed view use source defaults.
///
/// # Errors
/// Returns a deterministic rejection without modifying either borrowed input.
pub fn reduce(
    record: Option<&WorkflowRecord>,
    command: &CommittedCommand,
) -> Result<TransitionPlan, DomainError> {
    reduce_with_policy(record, command, None)
}

/// Decides a command while borrowing the current installed retry-only policy.
///
/// Only a fresh policy retry consumes this view. Legacy no-ops preserve the
/// original event and do not resolve newly supplied policy or timing inputs.
/// Policy installation, snapshot coherence and persistence are adapter duties.
///
/// # Errors
/// Returns source input/ownership errors or deterministic policy rejection.
pub fn reduce_with_policy(
    record: Option<&WorkflowRecord>,
    command: &CommittedCommand,
    installed_policy: Option<&InstalledRetryPolicy>,
) -> Result<TransitionPlan, DomainError> {
    reduce_profile(record, command, installed_policy, false)
}

/// Decides a command as the source's public mutation path does: a fresh
/// policy retry derives its jitter from the source hash of the command
/// instead of taking the supplied sample.
///
/// # Errors
/// Returns source input/ownership errors or deterministic policy rejection.
pub fn reduce_public(
    record: Option<&WorkflowRecord>,
    command: &CommittedCommand,
    installed_policy: Option<&InstalledRetryPolicy>,
) -> Result<TransitionPlan, DomainError> {
    reduce_profile(record, command, installed_policy, true)
}

fn reduce_profile(
    record: Option<&WorkflowRecord>,
    command: &CommittedCommand,
    installed_policy: Option<&InstalledRetryPolicy>,
    source_jitter: bool,
) -> Result<TransitionPlan, DomainError> {
    if command
        .id
        .as_bytes()
        .starts_with(b"__ferricstore_schedule__:")
    {
        return Err(DomainError::ReservedId);
    }
    if command.now.get() > MAX_EXACT_MILLIS {
        return Err(DomainError::NowTooLarge);
    }
    let fence = match &command.operation {
        Operation::Transition(transition) => Some(transition.fence),
        Operation::StepContinue(step) => Some(step.guard.fence),
        Operation::Complete(terminal) | Operation::Fail(terminal) => Some(terminal.guard.fence),
        Operation::Retry(retry) => Some(retry.guard.fence),
        Operation::RetryWithPolicy(retry) => Some(retry.guard.fence),
        Operation::ExtendLease(extend) => Some(extend.guard.fence),
        Operation::Cancel(cancel) => Some(cancel.fence),
        Operation::Create(_)
        | Operation::StartAndClaim(_)
        | Operation::Claim(_)
        | Operation::Reclaim(_) => None,
    };
    if fence.is_some_and(|fence| fence.get() > MAX_EXACT_MILLIS) {
        return Err(DomainError::FenceTooLarge);
    }
    if let Operation::RetryWithPolicy(retry) = &command.operation {
        retry.policy_override.validate()?;
        if let Some(run_at) = retry.run_at {
            validate_run_at(run_at)?;
        }
    }
    if record.is_some_and(|current| current.id != command.id) {
        return Err(DomainError::NotFound);
    }
    match &command.operation {
        Operation::Create(create) => create_record(record, command, create),
        Operation::StartAndClaim(start) => start_and_claim_record(record, command, start),
        Operation::Claim(claim) => claim_record(require_record(record)?, command, claim, false),
        Operation::Reclaim(claim) => claim_record(require_record(record)?, command, claim, true),
        Operation::Transition(transition) => {
            transition_record(require_record(record)?, command, transition)
        }
        Operation::Complete(terminal) => terminal_record(
            require_record(record)?,
            command,
            terminal,
            EventKind::Completed,
        ),
        Operation::Fail(terminal) => terminal_record(
            require_record(record)?,
            command,
            terminal,
            EventKind::Failed,
        ),
        Operation::Retry(retry) => retry_record(require_record(record)?, command, retry),
        Operation::RetryWithPolicy(retry) => policy_retry_record(
            require_record(record)?,
            command,
            retry,
            installed_policy,
            source_jitter,
        ),
        Operation::ExtendLease(extend) => extend_record(require_record(record)?, command, extend),
        Operation::Cancel(cancel) => cancel_record(require_record(record)?, command, cancel),
        Operation::StepContinue(step) => {
            step_continue_record(require_record(record)?, command, step)
        }
    }
}
