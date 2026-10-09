//! Source command admission and ownership preconditions.

use wf_types::version::{Fence, TimestampMillis};

use crate::lease::{LeaseGuard, LeaseToken};
use crate::value::{Counter, InputField};

use super::{DomainError, MAX_EXACT_MILLIS, WorkflowRecord};

pub(super) fn require_record(
    record: Option<&WorkflowRecord>,
) -> Result<&WorkflowRecord, DomainError> {
    record.ok_or(DomainError::NotFound)
}

pub(super) fn require_nonempty(bytes: &[u8], field: InputField) -> Result<(), DomainError> {
    if bytes.is_empty() {
        Err(DomainError::EmptyValue(field))
    } else {
        Ok(())
    }
}

pub(super) fn validate_run_at(time: TimestampMillis) -> Result<(), DomainError> {
    if time.get() > MAX_EXACT_MILLIS {
        Err(DomainError::RunAtTooLarge)
    } else {
        Ok(())
    }
}

pub(super) fn lease_deadline(
    now: TimestampMillis,
    lease_ms: u64,
) -> Result<TimestampMillis, DomainError> {
    if lease_ms == 0 {
        return Err(DomainError::ZeroLease);
    }
    if lease_ms > MAX_EXACT_MILLIS {
        return Err(DomainError::LeaseTooLarge);
    }
    now.checked_add(lease_ms)
        .filter(|deadline| deadline.get() <= MAX_EXACT_MILLIS)
        .ok_or(DomainError::LeaseDeadlineTooLarge)
}

pub(super) fn next_version(record: &WorkflowRecord) -> Result<u64, DomainError> {
    record
        .version
        .checked_add(1)
        .ok_or(DomainError::CounterOverflow(Counter::Version))
}

pub(super) fn terminal_state(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

pub(super) fn require_fence(record: &WorkflowRecord, fence: Fence) -> Result<(), DomainError> {
    if record.fence == fence {
        Ok(())
    } else {
        Err(DomainError::StaleLease)
    }
}

pub(super) fn require_running_lease(
    record: &WorkflowRecord,
    guard: &LeaseGuard,
) -> Result<(), DomainError> {
    if record.state == b"running"
        && record
            .lease
            .as_ref()
            .is_some_and(|lease| lease.token == guard.token)
    {
        require_fence(record, guard.fence)
    } else {
        Err(DomainError::StaleLease)
    }
}

pub(super) fn require_transition_lease(
    record: &WorkflowRecord,
    token: Option<&LeaseToken>,
) -> Result<(), DomainError> {
    if record.lease.as_ref().map(|lease| &lease.token) == token {
        Ok(())
    } else {
        Err(DomainError::StaleLease)
    }
}

pub(super) fn retry_run_state(record: &WorkflowRecord) -> &[u8] {
    record
        .run_state
        .as_deref()
        .filter(|state| !state.is_empty())
        .unwrap_or(b"queued")
}
