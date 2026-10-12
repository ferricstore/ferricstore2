//! Ordinary create and mutation of one explicitly selected claim candidate.

use wf_types::version::Fence;

use crate::claim::Claim;
use crate::history::{EventKind, LatestEvent};
use crate::lease::Lease;
use crate::value::{Counter, InputField};

use super::plan::{changed_plan, noop_plan};
use super::validation::{
    lease_deadline, next_version, require_nonempty, retry_run_state, terminal_state,
    validate_run_at,
};
use super::{CommittedCommand, Create, DomainError, Outcome, TransitionPlan, WorkflowRecord};

pub(super) fn create_record(
    record: Option<&WorkflowRecord>,
    command: &CommittedCommand,
    create: &Create,
) -> Result<TransitionPlan, DomainError> {
    require_nonempty(&create.workflow_type, InputField::Type)?;
    if create.workflow_type == b"__ferricstore_schedule" {
        return Err(DomainError::ReservedType);
    }
    require_nonempty(&create.state, InputField::State)?;
    if create.state == b"running" {
        return Err(DomainError::RunningTransition);
    }
    let run_at = create.run_at.unwrap_or(command.now);
    validate_run_at(run_at)?;
    if record.is_some() {
        return Err(DomainError::AlreadyExists);
    }
    let next = WorkflowRecord {
        id: command.id.clone(),
        workflow_type: create.workflow_type.clone(),
        state: create.state.clone(),
        version: 1,
        fence: Fence::new(0),
        attempts: 0,
        created_at: command.now,
        updated_at: command.now,
        next_run_at: Some(run_at),
        run_state: None,
        lease: None,
        payload: create.payload.clone(),
        result: None,
        error: None,
        latest_event: LatestEvent {
            kind: EventKind::Created,
            version: 1,
            fence: Fence::new(0),
        },
    };
    Ok(changed_plan(None, next, EventKind::Created, command.now))
}

pub(super) fn claim_record(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    claim: &Claim,
    reclaim: bool,
) -> Result<TransitionPlan, DomainError> {
    require_nonempty(&claim.expected_state, InputField::State)?;
    require_nonempty(&claim.worker, InputField::Worker)?;
    let deadline = lease_deadline(command.now, claim.lease_ms)?;
    let ownership_eligible = if reclaim {
        record.state == b"running" && record.lease.is_some()
    } else {
        record.state != b"running" && record.lease.is_none()
    };
    if record.state != claim.expected_state
        || !ownership_eligible
        || terminal_state(&record.state)
        || record.next_run_at.is_none_or(|run_at| run_at > command.now)
    {
        return Ok(noop_plan(Outcome::NotEligible));
    }
    let version = next_version(record)?;
    let fence = record
        .fence
        .checked_next()
        .ok_or(DomainError::CounterOverflow(Counter::Fence))?;
    let next = WorkflowRecord {
        id: record.id.clone(),
        workflow_type: record.workflow_type.clone(),
        state: b"running".to_vec(),
        version,
        fence,
        attempts: record.attempts,
        created_at: record.created_at,
        updated_at: command.now,
        next_run_at: Some(deadline),
        run_state: Some(if reclaim {
            retry_run_state(record).to_vec()
        } else {
            record.state.clone()
        }),
        lease: Some(Lease {
            worker: claim.worker.clone(),
            token: claim.token.clone(),
            deadline,
        }),
        payload: record.payload.clone(),
        result: record.result.clone(),
        error: record.error.clone(),
        latest_event: record.latest_event,
    };
    Ok(changed_plan(
        Some(record),
        next,
        EventKind::Claimed,
        command.now,
    ))
}
