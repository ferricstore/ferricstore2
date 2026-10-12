//! Depth-first internal cancellation, retaining source held-step visibility.

use super::super::admission::admit_group;
use super::super::{ActiveTimeoutRejection, TimeoutLimits};
use super::state::{ShadowGroup, TimeoutFrame, TimeoutTrace, current_child_status, current_group};
use super::{admit_visit, load_shadow};
use crate::claim::ScopedRecordId;
use crate::lineage::{
    ChildGroup, ChildResolution, ChildStatus, ParentClosed, mark_cancelled, running_children,
};
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use std::collections::BTreeSet;

type ClosedGroupPatch<'v> = (usize, Vec<ScopedRecordId<'v>>, bool);

pub(super) fn trace_closed_children(
    trace: &mut TimeoutTrace<'_>,
    owner: usize,
    held_step: usize,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let original = trace.owners[owner].original;
    let step = trace.steps[held_step];
    let terminal = matches!(step.state, b"completed" | b"failed" | b"cancelled");
    let suffix = trace.groups.len();
    let mut patches = Vec::new();
    let mut refs = Vec::new();
    let mut any_resolved = false;
    for (id, group) in original.child_groups() {
        if let Some((patch, mut current, resolved)) =
            closed_group_patch(trace, owner, id, group, held_step, work)?
        {
            patches.push(patch);
            any_resolved |= resolved;
            current.extend(refs);
            refs = current;
        }
    }
    if (!terminal && !any_resolved) || refs.is_empty() {
        trace.groups.truncate(suffix);
        return Ok(());
    }
    let mut unique = BTreeSet::new();
    let mut children = Vec::new();
    for id in refs {
        admit_visit(
            &mut trace.charge,
            id.partition_key.len() + id.id.len(),
            work,
        )?;
        if !unique.insert(id) {
            continue;
        }
        if let Some(child) = load_shadow(trace, id, false, limits, work)? {
            children.push(child);
        }
    }
    trace.frames.push(TimeoutFrame::FinishCancelledGroups {
        owner,
        held_step,
        patches,
    });
    for owner in children.into_iter().rev() {
        trace.frames.push(TimeoutFrame::CancelChild { owner });
    }
    Ok(())
}

pub(super) fn closed_group_patch<'v>(
    trace: &mut TimeoutTrace<'v>,
    owner: usize,
    id: &'v [u8],
    group: &'v ChildGroup,
    held_step: usize,
    work: TimeoutLimits,
) -> Result<Option<ClosedGroupPatch<'v>>, Result<ActiveTimeoutRejection, ApplyError>> {
    admit_group(&mut trace.charge, id.len(), group.children.len(), work)?;
    if group.on_parent_closed != ParentClosed::Cancel {
        return Ok(None);
    }
    let checkpoint = trace.steps[held_step].group_checkpoint;
    let prior = current_group(trace, owner, id, checkpoint, work)?;
    let summary = prior.map_or(group.summary, |i| trace.groups[i].summary);
    let resolved = prior.map_or(group.resolved, |i| trace.groups[i].resolved);
    let original = trace.owners[owner].original;
    let mut updates = Vec::new();
    let mut refs = Vec::new();
    for (child_id, member) in running_children(group) {
        let partition = member
            .partition
            .as_ref()
            .unwrap_or(original.partition())
            .as_bytes();
        admit_visit(
            &mut trace.charge,
            child_id.as_bytes().len() + partition.len(),
            work,
        )?;
        let status = current_child_status(trace, prior, child_id, member.status, work)?;
        if status == ChildStatus::Running {
            updates.push((
                child_id,
                ChildStatus::Cancelled,
                Some((ChildStatus::Cancelled, None, None)),
            ));
            refs.push(ScopedRecordId {
                partition_key: partition,
                id: child_id.as_bytes(),
            });
        }
    }
    if updates.is_empty() {
        return Ok(None);
    }
    let summary = mark_cancelled(
        summary,
        u64::try_from(updates.len()).map_err(|_| Err(ApplyError::InvalidAuthority))?,
    )
    .ok_or(Err(ApplyError::InvalidAuthority))?;
    let patch = trace.groups.len();
    trace.groups.push(ShadowGroup {
        owner,
        id,
        original: group,
        prior,
        summary,
        resolved: resolved.or(Some(ChildResolution::Failure)),
        updates,
    });
    Ok(Some((patch, refs, resolved.is_some())))
}
