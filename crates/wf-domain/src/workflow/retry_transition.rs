//! Legacy caller-resolved retries and authoritative installed-policy retries.

use wf_types::version::{Fence, TimestampMillis};

use crate::history::{EventKind, RetryHistory};
use crate::policy::{InstalledRetryPolicy, resolve};
use crate::retry::{PolicyRetry, Retry, RetryDecision, RetryTime};
use crate::value::Counter;

use super::plan::{changed_plan, changed_retry_plan, noop_plan};
use super::validation::{
    next_version, require_running_lease, retry_run_state, terminal_state, validate_run_at,
};
use super::{CommittedCommand, DomainError, Outcome, TransitionPlan, WorkflowRecord};

fn duplicate_retry(record: &WorkflowRecord, fence: Fence, run_at: Option<TimestampMillis>) -> bool {
    if record.lease.is_some() || record.fence != fence {
        return false;
    }
    let schedule_matches = record.state != b"running"
        && !terminal_state(&record.state)
        && match run_at {
            Some(run_at) => record.next_run_at == Some(run_at),
            None => {
                record.next_run_at.is_some()
                    && record.state == retry_run_state(record)
                    && record.attempts > 0
            }
        };
    let history_matches = record.latest_event.kind == EventKind::Retry
        && record.latest_event.version == record.version
        && record.latest_event.fence == fence;
    schedule_matches || history_matches
}

pub(super) fn retry_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    retry: &Retry,
) -> Result<TransitionPlan, DomainError> {
    if let RetryTime::Explicit(run_at) = retry.run_at {
        validate_run_at(run_at)?;
    }
    let requested = match retry.run_at {
        RetryTime::Explicit(time) => Some(time),
        RetryTime::PolicyResolved(_) => None,
    };
    if duplicate_retry(record, retry.guard.fence, requested) {
        return Ok(noop_plan(Outcome::LegacyNoop));
    }
    require_running_lease(record, &retry.guard)?;
    validate_run_at(retry.run_at.committed())?;
    let attempts = record
        .attempts
        .checked_add(1)
        .ok_or(DomainError::CounterOverflow(Counter::Attempts))?;
    let mut next = record.clone();
    next.version = next_version(record)?;
    next.state = retry_run_state(record).to_vec();
    next.run_state = None;
    next.attempts = attempts;
    next.updated_at = command.now;
    next.next_run_at = Some(retry.run_at.committed());
    next.lease = None;
    next.error.clone_from(&retry.error);
    if let Some(payload) = &retry.payload {
        next.payload = Some(payload.clone());
    }
    Ok(changed_plan(
        Some(record),
        next,
        EventKind::Retry,
        command.now,
    ))
}

pub(super) fn policy_retry_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    retry: &PolicyRetry,
    installed_policy: Option<&InstalledRetryPolicy>,
) -> Result<TransitionPlan, DomainError> {
    if duplicate_retry(record, retry.guard.fence, retry.run_at) {
        return Ok(noop_plan(Outcome::LegacyNoop));
    }
    require_running_lease(record, &retry.guard)?;
    let run_state = retry_run_state(record);
    let policy = resolve(
        installed_policy,
        &record.workflow_type,
        run_state,
        &retry.policy_override,
    )?;
    let attempts = record
        .attempts
        .checked_add(1)
        .ok_or(DomainError::CounterOverflow(Counter::Attempts))?;
    let (state, next_run_at, decision) = if policy.attempt_allowed(attempts) {
        let run_at = match retry.run_at {
            Some(time) => time,
            None => policy.next_run_at(command.now, attempts, retry.jitter)?,
        };
        (run_state.to_vec(), Some(run_at), RetryDecision::Scheduled)
    } else {
        let due = if terminal_state(&policy.exhausted_to) {
            None
        } else {
            Some(command.now)
        };
        (policy.exhausted_to.clone(), due, RetryDecision::Exhausted)
    };
    let mut next = record.clone();
    next.version = next_version(record)?;
    next.state = state;
    next.run_state = None;
    next.attempts = attempts;
    next.updated_at = command.now;
    next.next_run_at = next_run_at;
    next.lease = None;
    next.error.clone_from(&retry.error);
    if let Some(payload) = &retry.payload {
        next.payload = Some(payload.clone());
    }
    let meta = RetryHistory {
        decision,
        run_state: run_state.to_vec(),
        next_run_at,
        policy,
    };
    Ok(changed_retry_plan(record, next, command.now, meta))
}
