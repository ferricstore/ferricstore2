//! Private closure trace with original-authority references only.

use super::super::admission::admit_group;
use super::super::{ActiveTimeoutRejection, TimeoutLimits};
use super::state::{ShadowGroup, TimeoutFrame, TimeoutStep, TimeoutTrace, TraceValueRef};
use super::{admit_step, admit_visit, load_shadow, release_occurrences};
use crate::claim::ScopedRecordId;
use crate::history::EventKind;
use crate::lineage::{ChildGroup, ChildResolution, ChildStatus, count_terminal, resolve_group};
use crate::value::{Counter, DomainError};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use wf_types::version::TimestampMillis;

pub(super) fn trace_parent(
    trace: &mut TimeoutTrace<'_>,
    child: usize,
    held_step: usize,
    status: ChildStatus,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let original_child = trace.owners[child].original;
    let Some(link) = original_child.parent().filter(|p| !p.id.is_empty()) else {
        return Ok(());
    };
    let id = ScopedRecordId {
        partition_key: link
            .partition
            .as_ref()
            .unwrap_or(original_child.partition())
            .as_bytes(),
        id: &link.id,
    };
    let Some(owner) = load_shadow(trace, id, false, limits, work)? else {
        return Ok(());
    };
    let original = trace.owners[owner].original;
    let child_step = trace.steps[held_step];
    for (id, group) in original.child_groups() {
        admit_group(&mut trace.charge, id.len(), group.children.len(), work)?;
        let checkpoint = trace.owners[owner].group_updates.len();
        let prior = super::state::current_group(trace, owner, id, checkpoint, work)?;
        let summary = prior.map_or(group.summary, |i| trace.groups[i].summary);
        let resolved = prior.map_or(group.resolved, |i| trace.groups[i].resolved);
        if resolved.is_some() {
            continue;
        }
        admit_group(
            &mut trace.charge,
            original_child.core().id().as_bytes().len(),
            1,
            work,
        )?;
        let Some((member_id, member)) = group.children.get_key_value(original_child.core().id())
        else {
            continue;
        };
        let member_status =
            super::state::current_child_status(trace, prior, member_id, member.status, work)?;
        if member_status != ChildStatus::Running {
            continue;
        }
        admit_parent_path(trace, owner, work)?;
        let summary = count_terminal(summary, status).ok_or(Err(ApplyError::InvalidAuthority))?;
        let resolved = resolve_group(group.wait, group.on_child_failed, summary, None, status);
        let result = original_child
            .values()
            .result
            .as_ref()
            .map(TraceValueRef::Original);
        let patch = trace.groups.len();
        trace.groups.push(ShadowGroup {
            owner,
            id,
            original: group,
            prior,
            summary,
            resolved,
            updates: vec![(
                member_id,
                status,
                Some((status, result, child_step.error_ref)),
            )],
        });
        trace.owners[owner].group_updates.push(patch);
        let step = parent_step(trace, owner, group, resolved, status, work)?;
        let held_step = trace.steps.len();
        trace.steps.push(step);
        trace.owners[owner].latest_step = Some(held_step);
        if let Some(reservation) = release_occurrences(step.state, step.held_reservation) {
            trace.releases.push((owner, reservation, trace.now));
        }
        trace.frames.extend([
            TimeoutFrame::LeavePath { owner },
            TimeoutFrame::PropagateParent { owner, held_step },
            TimeoutFrame::CancelClosed { owner, held_step },
        ]);
        break;
    }
    Ok(())
}

fn admit_parent_path(
    trace: &mut TimeoutTrace<'_>,
    owner: usize,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let original = trace.owners[owner].original;
    let id = ScopedRecordId {
        partition_key: original.partition().as_bytes(),
        id: original.core().id().as_bytes(),
    };
    admit_visit(
        &mut trace.charge,
        id.partition_key.len() + id.id.len(),
        work,
    )?;
    if !trace.path.insert(id) {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    Ok(())
}

pub(super) fn parent_step<'v>(
    trace: &mut TimeoutTrace<'v>,
    owner: usize,
    group: &'v ChildGroup,
    resolved: Option<ChildResolution>,
    status: ChildStatus,
    work: TimeoutLimits,
) -> Result<TimeoutStep<'v>, Result<ActiveTimeoutRejection, ApplyError>> {
    let original = trace.owners[owner].original;
    let previous = trace.owners[owner].latest_step;
    let old_state = previous.map_or(original.core().state(), |i| trace.steps[i].state);
    let state = match resolved {
        Some(ChildResolution::Success) => group.success_state.as_slice(),
        Some(ChildResolution::Failure) => group.failure_state.as_slice(),
        None => old_state,
    };
    let terminal = matches!(state, b"completed" | b"failed" | b"cancelled");
    let mut ordering = previous.map_or(original.ordering(), |i| trace.steps[i].ordering);
    let logical = if old_state == b"running" {
        original.core().run_state().unwrap_or(b"queued")
    } else {
        old_state
    };
    let next_logical = if state == b"running" {
        original.core().run_state().unwrap_or(b"queued")
    } else {
        state
    };
    admit_visit(&mut trace.charge, next_logical.len() + logical.len(), work)?;
    if next_logical != logical && !terminal {
        let ordinal = trace
            .next_ordinal
            .ok_or(Err(ApplyError::InvalidAuthority))?;
        ordering.state_enter_seq =
            (u128::from(trace.order.identity.index().get()) << 32) + u128::from(ordinal);
        trace.next_ordinal = ordinal.checked_add(1);
        trace.consumed_ordinals += 1;
    }
    let version = previous
        .map_or(original.core().version(), |i| trace.steps[i].version)
        .checked_add(1)
        .ok_or(Ok(ActiveTimeoutRejection::Domain(
            DomainError::CounterOverflow(Counter::Version),
        )))?;
    let next_run_at = previous.map_or(original.core().next_run_at(), |i| {
        trace.steps[i].next_run_at
    });
    let lease = previous.map_or(original.core().lease(), |i| trace.steps[i].lease);
    let ttl_ms = previous.map_or(original.ttl_ms(), |i| trace.steps[i].ttl_ms);
    let retained = TimestampMillis::new(
        trace
            .now
            .get()
            .saturating_add(original.captured().retention_ttl_ms())
            .min(MAX_EXACT_MILLIS),
    );
    let error_ref = previous.map_or(
        original
            .values()
            .error
            .as_ref()
            .map(TraceValueRef::Original),
        |i| trace.steps[i].error_ref,
    );
    let step = TimeoutStep {
        owner,
        previous,
        group_checkpoint: trace.owners[owner].group_updates.len(),
        kind: match status {
            ChildStatus::Completed => EventKind::ChildCompleted,
            ChildStatus::Failed => EventKind::ChildFailed,
            ChildStatus::Cancelled => EventKind::ChildCancelled,
            ChildStatus::Running => return Err(Err(ApplyError::InvalidAuthority)),
        },
        version,
        state,
        updated_at: trace.now,
        next_run_at: if resolved.is_some() {
            None
        } else {
            next_run_at
        },
        lease: if resolved.is_some() { None } else { lease },
        ttl_ms: if resolved.is_some() { None } else { ttl_ms },
        retention_until: terminal.then_some(retained),
        error_ref,
        timeout: None,
        ordering,
        held_reservation: if previous.is_none_or(|i| trace.steps[i].state == b"running") {
            original.governance()
        } else {
            None
        },
    };
    admit_step(trace, step, work)?;
    Ok(step)
}
