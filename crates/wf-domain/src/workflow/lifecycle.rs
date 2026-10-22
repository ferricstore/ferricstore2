//! Nonterminal, terminal, lease-extension and cancellation mutations.

use wf_types::version::{Fence, TimestampMillis};

use crate::history::EventKind;
use crate::lease::{ExtendLease, LeaseToken};
use crate::value::{Counter, InputField};

use super::plan::{changed_plan, noop_plan};
use super::validation::{
    lease_deadline, next_version, require_fence, require_nonempty, require_running_lease,
    require_transition_lease, retry_run_state, terminal_state, validate_run_at,
};
use super::{
    Cancel, CommittedCommand, DomainError, MAX_EXACT_MILLIS, Outcome, StepContinue, Terminal,
    Transition, TransitionPlan, WorkflowRecord,
};

pub(super) fn transition_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    transition: &Transition,
) -> Result<TransitionPlan, DomainError> {
    require_nonempty(&transition.from, InputField::From)?;
    require_nonempty(&transition.to, InputField::To)?;
    if transition.to == b"running" {
        return Err(DomainError::RunningTransition);
    }
    let run_at = transition.run_at.unwrap_or(command.now);
    validate_run_at(run_at)?;
    if record.state == transition.to
        && !terminal_state(&transition.to)
        && record.lease.is_none()
        && record.fence == transition.fence
    {
        return Ok(noop_plan(Outcome::LegacyNoop));
    }
    if record.state != transition.from {
        return Err(DomainError::WrongState);
    }
    if terminal_state(&record.state) {
        return Err(DomainError::Terminal);
    }
    require_fence(record, transition.fence)?;
    require_transition_lease(record, transition.lease_token.as_ref())?;
    if terminal_state(&transition.to) {
        return Err(DomainError::TerminalTransition);
    }
    let mut next = record.clone();
    next.version = next_version(record)?;
    next.state.clone_from(&transition.to);
    next.updated_at = command.now;
    next.next_run_at = Some(run_at);
    next.lease = None;
    if let Some(payload) = &transition.payload {
        next.payload = Some(payload.clone());
    }
    Ok(changed_plan(
        Some(record),
        next,
        EventKind::Transitioned,
        command.now,
    ))
}

pub(super) fn step_continue_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    step: &StepContinue,
) -> Result<TransitionPlan, DomainError> {
    require_nonempty(&step.from, InputField::From)?;
    require_nonempty(&step.to, InputField::To)?;
    if step.to == b"running" {
        return Err(DomainError::RunningTransition);
    }
    let deadline = lease_deadline(command.now, step.lease_ms)?;
    require_running_lease(record, &step.guard)?;
    if retry_run_state(record) != step.from.as_slice() {
        return Err(DomainError::WrongState);
    }
    let lease = record.lease.as_ref().ok_or(DomainError::StaleLease)?;
    if lease.worker.is_empty()
        || step
            .worker
            .as_ref()
            .is_some_and(|worker| worker.as_slice() != lease.worker.as_slice())
    {
        return Err(DomainError::StaleLease);
    }
    if terminal_state(&step.to) {
        return Err(DomainError::TerminalTransition);
    }
    let version = next_version(record)?;
    let fence = record
        .fence
        .checked_next()
        .ok_or(DomainError::CounterOverflow(Counter::Fence))?;
    let token = step_token(&lease.worker, command.now, fence)?;
    let mut next = record.clone();
    next.version = version;
    next.fence = fence;
    next.run_state = Some(step.to.clone());
    next.updated_at = command.now;
    next.next_run_at = Some(deadline);
    let lease = next.lease.as_mut().ok_or(DomainError::StaleLease)?;
    lease.token = token;
    lease.deadline = deadline;
    if let Some(payload) = &step.payload {
        next.payload = Some(payload.clone());
    }
    Ok(changed_plan(
        Some(record),
        next,
        EventKind::StepContinued,
        command.now,
    ))
}

fn step_token(
    worker: &[u8],
    now: TimestampMillis,
    fence: Fence,
) -> Result<LeaseToken, DomainError> {
    let mut bytes = worker.to_vec();
    bytes.push(b':');
    bytes.extend_from_slice(now.get().to_string().as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(fence.get().to_string().as_bytes());
    LeaseToken::new(bytes)
}

pub(super) fn terminal_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    terminal: &Terminal,
    kind: EventKind,
) -> Result<TransitionPlan, DomainError> {
    let state = match kind {
        EventKind::Completed => b"completed".as_slice(),
        EventKind::Failed => b"failed".as_slice(),
        _ => return Err(DomainError::UnsupportedOperation),
    };
    if record.state == state && record.fence == terminal.guard.fence {
        return Ok(noop_plan(Outcome::LegacyNoop));
    }
    require_running_lease(record, &terminal.guard)?;
    let mut next = record.clone();
    next.version = next_version(record)?;
    next.state = state.to_vec();
    next.updated_at = command.now;
    next.next_run_at = None;
    next.lease = None;
    if let Some(payload) = &terminal.payload {
        next.payload = Some(payload.clone());
    }
    if kind == EventKind::Completed {
        next.result.clone_from(&terminal.value);
    } else {
        next.error.clone_from(&terminal.value);
    }
    Ok(changed_plan(Some(record), next, kind, command.now))
}

pub(super) fn extend_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    extend: &ExtendLease,
) -> Result<TransitionPlan, DomainError> {
    let deadline = lease_deadline(command.now, extend.lease_ms)?;
    require_running_lease(record, &extend.guard)?;
    let mut next = record.clone();
    next.version = next_version(record)?;
    next.updated_at = command.now;
    next.next_run_at = Some(deadline);
    next.lease.as_mut().ok_or(DomainError::StaleLease)?.deadline = deadline;
    Ok(changed_plan(
        Some(record),
        next,
        EventKind::LeaseExtended,
        command.now,
    ))
}

pub(super) fn cancel_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    cancel: &Cancel,
) -> Result<TransitionPlan, DomainError> {
    if let Some(expected_state) = &cancel.expected_state {
        require_nonempty(expected_state, InputField::ExpectState)?;
    }
    if cancel
        .expected_version
        .is_some_and(|version| version > MAX_EXACT_MILLIS)
    {
        return Err(DomainError::ExpectedVersionTooLarge);
    }
    if cancel
        .expected_version
        .is_some_and(|version| version != record.version)
    {
        return Err(DomainError::WrongVersion);
    }
    if cancel
        .expected_state
        .as_ref()
        .is_some_and(|state| *state != record.state)
    {
        return Err(DomainError::WrongState);
    }
    require_fence(record, cancel.fence)?;
    if terminal_state(&record.state) {
        return Err(DomainError::Terminal);
    }
    require_transition_lease(record, cancel.lease_token.as_ref())?;
    let mut next = record.clone();
    next.version = next_version(record)?;
    next.state = b"cancelled".to_vec();
    next.updated_at = command.now;
    next.next_run_at = None;
    next.lease = None;
    next.error.clone_from(&cancel.error);
    Ok(changed_plan(
        Some(record),
        next,
        EventKind::Cancelled,
        command.now,
    ))
}
