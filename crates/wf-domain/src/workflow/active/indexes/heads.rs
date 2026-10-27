//! Preplanned FIFO heads from the original authority and scalar touched overlay.

use super::super::admission::admit_visit;
use super::super::authority::{validate_consumed_record, validate_dependency_placement};
use super::super::trace::state::TimeoutTrace;
use super::super::{ActiveRecord, ActiveTimeoutRejection, AdmissionLimits, TimeoutLimits};
use crate::claim::{ClaimRecord, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::admission::record_bytes;
use crate::workflow::batch::ApplyError;

type OverlayHead<'v> = Option<(Option<usize>, &'v ActiveRecord)>;

pub(in super::super) fn plan_fifo_heads(
    trace: &mut TimeoutTrace<'_>,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    for owner in 0..trace.owners.len() {
        let Some(position) = trace.owners[owner].latest_step else {
            continue;
        };
        let original = trace.owners[owner].original;
        let before = ClaimRecord {
            record: original.core(),
            partition_key: original.partition().as_bytes(),
            priority: original.ordering().priority,
            state_enter_seq: original.ordering().state_enter_seq,
        };
        let step = trace.steps[position];
        let after_state = if step.state == b"running" {
            original.core().run_state().unwrap_or(b"queued")
        } else {
            step.state
        };
        let after_lane = (!matches!(after_state, b"completed" | b"failed" | b"cancelled"))
            .then_some(LaneKey {
                state: after_state,
                workflow_type: original.core().workflow_type(),
                partition_key: original.partition().as_bytes(),
            });
        for lane in [LaneKey::for_record(&before), after_lane]
            .into_iter()
            .flatten()
        {
            observe_lane(trace, lane, limits, work)?;
        }
    }
    Ok(())
}

fn observe_lane<'v>(
    trace: &mut TimeoutTrace<'v>,
    lane: LaneKey<'v>,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let names = lane.workflow_type.len() + lane.state.len() + lane.partition_key.len();
    admit_visit(&mut trace.charge, names, work)?;
    let mode = trace
        .view
        .indexed
        .policies
        .get(lane.workflow_type)
        .map_or(StateMode::Parallel, |policy| policy.state_mode(lane.state));
    if mode != StateMode::Fifo {
        return Ok(());
    }
    let mut known = false;
    for (old, _) in &trace.heads {
        let old_names = old.workflow_type.len() + old.state.len() + old.partition_key.len();
        admit_visit(&mut trace.charge, names + old_names, work)?;
        if *old == lane {
            known = true;
            break;
        }
    }
    if !known {
        let head = choose_overlay_head(trace, lane, limits, work)?;
        trace.heads.push((lane, head));
    }
    Ok(())
}

pub(in super::super) fn choose_overlay_head<'v>(
    trace: &mut TimeoutTrace<'v>,
    lane: LaneKey<'v>,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<OverlayHead<'v>, Result<ActiveTimeoutRejection, ApplyError>> {
    let invalid = || Err(ApplyError::InvalidAuthority);
    let lane_names = lane.workflow_type.len() + lane.state.len() + lane.partition_key.len();
    let mut head = None;
    if let Some(members) = trace.view.indexed.fifo.get(&lane) {
        for (order, member) in members {
            limits.check_value(order.id()).map_err(|_| invalid())?;
            let r = record_bytes(member.record, limits).map_err(|_| invalid())?;
            let names = member.partition_key.len() + order.id().len();
            admit_visit(
                &mut trace.charge,
                r.checked_add(names).ok_or_else(invalid)?,
                work,
            )?;
            let id = ScopedRecordId::for_record(member);
            let record = trace.view.records.get(&id).copied().ok_or_else(invalid)?;
            validate_overlay_head(trace, record, limits, work)?;
            let expected = ClaimRecord {
                record: record.core(),
                partition_key: record.partition().as_bytes(),
                priority: record.ordering().priority,
                state_enter_seq: record.ordering().state_enter_seq,
            };
            let bytes = record_bytes(record.core(), limits)
                .map_err(|_| invalid())?
                .checked_add(r)
                .and_then(|n| n.checked_add(names))
                .and_then(|n| n.checked_add(record.core().id().as_bytes().len()))
                .and_then(|n| n.checked_add(lane_names))
                .ok_or_else(invalid)?;
            admit_visit(&mut trace.charge, bytes, work)?;
            if *member != expected
                || *order != LaneOrder::for_record(&expected)
                || LaneKey::for_record(&expected) != Some(lane)
            {
                return Err(invalid());
            }
            let changed = trace.lookup.get(&id).copied();
            if changed.is_some_and(|i| trace.owners[i].latest_step.is_some()) {
                continue;
            }
            head = Some((None, record));
            break;
        }
    }
    let mut best = head.map(|(_, record): (Option<usize>, &ActiveRecord)| {
        (
            if record.core().state() == b"running" {
                -1_i8
            } else {
                0
            },
            record.ordering().state_enter_seq,
            record.core().id().as_bytes(),
        )
    });
    for (owner, candidate) in trace.owners.iter().enumerate() {
        admit_visit(&mut trace.charge, 0, work)?;
        let Some(position) = candidate.latest_step else {
            continue;
        };
        let step = trace.steps[position];
        let original = candidate.original;
        let state = if step.state == b"running" {
            original.core().run_state().unwrap_or(b"queued")
        } else {
            step.state
        };
        let names = original.core().workflow_type().len()
            + original.partition().as_bytes().len()
            + state.len()
            + original.core().id().as_bytes().len()
            + lane_names
            + best.map_or(0, |b| b.2.len());
        admit_visit(&mut trace.charge, names, work)?;
        if state != lane.state
            || original.partition().as_bytes() != lane.partition_key
            || original.core().workflow_type() != lane.workflow_type
        {
            continue;
        }
        let order = (
            if step.state == b"running" { -1_i8 } else { 0 },
            step.ordering.state_enter_seq,
            original.core().id().as_bytes(),
        );
        if best.is_none_or(|old| order < old) {
            head = Some((Some(owner), original));
            best = Some(order);
        }
    }
    Ok(head)
}

pub(in super::super) fn validate_overlay_head(
    trace: &mut TimeoutTrace<'_>,
    record: &ActiveRecord,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    admit_visit(&mut trace.charge, record.partition().as_bytes().len(), work)?;
    validate_dependency_placement(trace.view, record).map_err(Err)?;
    validate_consumed_record(trace.view, record, limits, &mut trace.charge, work).map(|_| ())
}
