//! Exact moved consumed-authority guard; optional-due correction follows real RED.

use super::super::{ClaimApplyView, ClaimRejection, ClaimWorkLimits};
use super::ClaimPreflightError;
use crate::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::admission::{AdmissionLimits, record_bytes};
use crate::workflow::batch::ApplyError;
use wf_types::bounds::{MAX_API_KEY_BYTES, MAX_RECORD_ID_BYTES};

pub(in super::super) fn guard_consumed(
    view: ClaimApplyView<'_>,
    entry: ClaimRecord<'_>,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
) -> Result<(), ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let overflow = ClaimPreflightError::Business(ClaimRejection::LengthOverflow);
    let r = record_bytes(entry.record, limits).map_err(|_| invalid)?;
    let names = admit_authority_components(entry, limits)?;
    let id = ScopedRecordId::for_record(&entry);
    let index = view.group.records.get(&id).ok_or(invalid)?;
    let canonical = view.group.batch.records.get(&id).ok_or(invalid)?;
    let ri = record_bytes(index.record, limits).map_err(|_| invalid)?;
    let rg = record_bytes(canonical, limits).map_err(|_| invalid)?;
    admit_authority_components(*index, limits)?;
    admit_authority_components(
        ClaimRecord {
            record: canonical,
            ..entry
        },
        limits,
    )?;
    let d = r
        .checked_mul(3)
        .and_then(|n| ri.checked_mul(2).and_then(|i| n.checked_add(i)))
        .and_then(|n| n.checked_add(rg))
        .ok_or(overflow)?;
    let next = u64::try_from(names)
        .ok()
        .and_then(|n| n.checked_mul(256))
        .and_then(|n| u64::try_from(d).ok().and_then(|d| n.checked_add(d)))
        .and_then(|n| g.checked_add(n))
        .ok_or(overflow)?;
    if next > work.max_work_bytes() {
        return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
            bytes: next,
            maximum: work.max_work_bytes(),
        }));
    }
    *g = next;
    let encoding = view.encodings.get(&id).ok_or(invalid)?;
    if entry.priority > 2
        || encoding.partition.as_bytes() != entry.partition_key
        || encoding.state_key.len() > MAX_API_KEY_BYTES
        || view.group.batch.placements.get(encoding.partition) != Some(view.group.batch.group)
        || index != &entry
        || *canonical != entry.record
    {
        return Err(invalid);
    }
    let policy = view.group.policies.get(entry.record.workflow_type());
    if policy.is_some_and(|p| {
        p.workflow_type().len() > limits.inline_bytes()
            || p.workflow_type() != entry.record.workflow_type()
    }) {
        return Err(invalid);
    }
    let mode = policy
        .and_then(|p| p.states().get(entry.logical_state()))
        .and_then(|s| s.mode)
        .unwrap_or(StateMode::Parallel);
    let lane = LaneKey::for_record(&entry).ok_or(invalid)?;
    let due = DueOrder::for_record(&entry);
    match mode {
        StateMode::Parallel => {
            let due = due.ok_or(invalid)?;
            if view.group.fifo.contains_key(&lane) || !view.group.parallel.contains_key(&due) {
                return Err(invalid);
            }
        }
        StateMode::Fifo => {
            if entry.priority != 0
                || due.is_some_and(|due| view.group.parallel.contains_key(&due))
                || !view
                    .group
                    .fifo
                    .get(&lane)
                    .is_some_and(|members| members.contains_key(&LaneOrder::for_record(&entry)))
            {
                return Err(invalid);
            }
        }
    }
    Ok(())
}

fn admit_authority_components(
    entry: ClaimRecord<'_>,
    limits: AdmissionLimits,
) -> Result<usize, ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let mut names = 0_usize;
    for (component, typed) in [
        (entry.record.id().as_bytes(), true),
        (entry.record.workflow_type(), false),
        (entry.record.state(), false),
        (entry.logical_state(), false),
        (entry.partition_key, true),
    ] {
        if component.is_empty()
            || (!typed && component.len() > limits.inline_bytes())
            || (typed && component.len() > MAX_RECORD_ID_BYTES)
        {
            return Err(invalid);
        }
        if component.len() > MAX_RECORD_ID_BYTES {
            return Err(ClaimPreflightError::Business(
                ClaimRejection::ValueTooLarge {
                    bytes: component.len(),
                    maximum: MAX_RECORD_ID_BYTES,
                },
            ));
        }
        names = names
            .checked_add(component.len())
            .ok_or(ClaimPreflightError::Business(
                ClaimRejection::LengthOverflow,
            ))?;
    }
    Ok(names)
}
