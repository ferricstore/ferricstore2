//! Actual-length, represented-read and complete pre-copy claim admission.

use wf_types::bounds::MAX_RECORD_ID_BYTES;
use wf_types::version::TimestampMillis;

use crate::claim::request::{MAX_CLAIMS, MAX_HEADS, MAX_SCANS};
use crate::claim::{DueOrder, LaneKey, PartitionFilter};
use crate::policy::definition::StateMode;
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::indexed::TouchedIndexedGroupView;

mod consumed;
mod selected;
use super::{ClaimEnvelope, ClaimQuery, ClaimRejection, ClaimStates, ClaimWorkLimits};
pub(super) use consumed::{guard_consumed, guard_consumed_with};
pub(super) use selected::{preflight_selected, preflight_selected_with};

#[derive(Clone, Copy, Debug)]
pub(super) enum ClaimPreflightError {
    Business(ClaimRejection),
    Apply(ApplyError),
}

pub(super) fn admit_query(
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
) -> Result<(usize, u64), ClaimPreflightError> {
    admit_query_with_prior(envelope, now, limits, work, 0)
}

pub(super) fn admit_query_with_prior(
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    prior_work: u64,
) -> Result<(usize, u64), ClaimPreflightError> {
    let query = envelope.query;
    let partitions = match query.partitions {
        PartitionFilter::Any => &[][..],
        PartitionFilter::Only(p) => p,
    };
    let states = match query.states {
        ClaimStates::Any => &[][..],
        ClaimStates::Only(s) => s,
    };
    admit_query_counts(envelope, now, limits)?;
    let overflow = ClaimPreflightError::Business(ClaimRejection::LengthOverflow);
    let mut q = 0_usize;
    let mut g = u64::try_from(envelope.header.encoded_entry.len())
        .map_err(|_| overflow)?
        .checked_add(prior_work)
        .ok_or(overflow)?;
    for (components, maximum) in [
        (&[query.workflow_type][..], MAX_RECORD_ID_BYTES),
        (&[query.worker][..], limits.inline_bytes()),
        (partitions, MAX_RECORD_ID_BYTES),
        (states, MAX_RECORD_ID_BYTES),
        (query.exclude_states, MAX_RECORD_ID_BYTES),
    ] {
        for component in components {
            if component.is_empty() {
                return Err(ClaimPreflightError::Business(empty_component(&query)));
            }
            let maximum = maximum.min(limits.inline_bytes());
            if component.len() > maximum {
                return Err(ClaimPreflightError::Business(
                    ClaimRejection::ValueTooLarge {
                        bytes: component.len(),
                        maximum,
                    },
                ));
            }
            q = q.checked_add(component.len()).ok_or(overflow)?;
            g = g
                .checked_add(
                    u64::try_from(component.len())
                        .ok()
                        .and_then(|n| n.checked_mul(256))
                        .ok_or(overflow)?,
                )
                .ok_or(overflow)?;
            if q > limits.decision_bytes() {
                return Err(ClaimPreflightError::Business(ClaimRejection::DecisionBytes));
            }
            if g > work.max_work_bytes() {
                return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
                    bytes: g,
                    maximum: work.max_work_bytes(),
                }));
            }
        }
    }
    for components in [partitions, states, query.exclude_states] {
        for (index, component) in components.iter().enumerate() {
            if components[..index].contains(component) {
                return Err(ClaimPreflightError::Business(ClaimRejection::InvalidScope));
            }
        }
    }
    if query.priority.is_some_and(|p| p > 2) {
        return Err(ClaimPreflightError::Business(
            ClaimRejection::InvalidPriority,
        ));
    }
    if query.lease_ms == 0 {
        return Err(ClaimPreflightError::Business(ClaimRejection::ZeroLease));
    }
    if query.lease_ms > MAX_EXACT_MILLIS {
        return Err(ClaimPreflightError::Business(ClaimRejection::LeaseTooLarge));
    }
    if now
        .checked_add(query.lease_ms)
        .is_none_or(|n| n.get() > MAX_EXACT_MILLIS)
    {
        return Err(ClaimPreflightError::Business(
            ClaimRejection::LeaseDeadlineTooLarge,
        ));
    }
    Ok((q, g))
}

fn empty_component(query: &ClaimQuery<'_>) -> ClaimRejection {
    if query.workflow_type.is_empty() {
        ClaimRejection::EmptyType
    } else if query.worker.is_empty() {
        ClaimRejection::EmptyWorker
    } else {
        ClaimRejection::InvalidScope
    }
}

fn admit_query_counts(
    envelope: &ClaimEnvelope<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<(), ClaimPreflightError> {
    let query = envelope.query;
    let partitions = match query.partitions {
        PartitionFilter::Any => &[][..],
        PartitionFilter::Only(p) => p,
    };
    let states = match query.states {
        ClaimStates::Any => &[][..],
        ClaimStates::Only(s) => s,
    };
    if matches!(query.partitions, PartitionFilter::Only([]))
        || matches!(query.states, ClaimStates::Only([]))
        || partitions.len() > 64
        || states.len() > 64
        || query.exclude_states.len() > 64
        || partitions.len().max(1) * states.len().max(1) > 64
    {
        return Err(ClaimPreflightError::Business(ClaimRejection::InvalidScope));
    }
    if !(1..=MAX_CLAIMS.min(limits.batch_items())).contains(&query.limit)
        || !(1..=MAX_SCANS).contains(&envelope.scan_budget)
        || !(1..=MAX_HEADS).contains(&envelope.head_budget)
        || now.get() > MAX_EXACT_MILLIS
    {
        return Err(ClaimPreflightError::Business(ClaimRejection::InvalidBudget));
    }
    Ok(())
}

pub(super) fn guard_lane(
    lane: LaneKey<'_>,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
) -> Result<(), ClaimPreflightError> {
    let mut names = 0_usize;
    for (component, typed) in [
        (lane.workflow_type, false),
        (lane.state, false),
        (lane.partition_key, true),
    ] {
        if component.is_empty()
            || (!typed && component.len() > limits.inline_bytes())
            || (typed && component.len() > MAX_RECORD_ID_BYTES)
        {
            return Err(ClaimPreflightError::Apply(ApplyError::InvalidAuthority));
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
    let next = u64::try_from(names)
        .ok()
        .and_then(|n| n.checked_mul(256))
        .and_then(|n| n.checked_add(1_024))
        .and_then(|n| g.checked_add(n))
        .ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
    if next > work.max_work_bytes() {
        return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
            bytes: next,
            maximum: work.max_work_bytes(),
        }));
    }
    *g = next;
    Ok(())
}

pub(super) fn guard_observed_lane(
    view: TouchedIndexedGroupView<'_, '_>,
    lane: LaneKey<'_>,
    query: ClaimQuery<'_>,
    limits: AdmissionLimits,
) -> Result<(), ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let partition = match query.partitions {
        PartitionFilter::Any => true,
        PartitionFilter::Only(partitions) => partitions.contains(&lane.partition_key),
    };
    let state = match query.states {
        ClaimStates::Any => true,
        ClaimStates::Only(states) => {
            states.contains(&lane.state) || states.contains(&b"running".as_slice())
        }
    };
    if lane.workflow_type != query.workflow_type
        || !partition
        || !state
        || !view.placements.iter().any(|(partition, group)| {
            partition.as_bytes() == lane.partition_key && group == view.group
        })
    {
        return Err(invalid);
    }
    let policy = view
        .policies
        .get(lane.workflow_type)
        .copied()
        .flatten()
        .ok_or(invalid)?;
    if policy.workflow_type() != lane.workflow_type
        || policy.workflow_type().len() > limits.inline_bytes()
        || policy.states().get(lane.state).and_then(|state| state.mode) != Some(StateMode::Fifo)
    {
        return Err(invalid);
    }
    Ok(())
}

pub(super) fn guard_due(
    due: DueOrder<'_>,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
) -> Result<(), ClaimPreflightError> {
    if due.priority > 2 || due.due.get() > MAX_EXACT_MILLIS {
        return Err(ClaimPreflightError::Apply(ApplyError::InvalidAuthority));
    }
    let mut names = 0_usize;
    for (component, typed) in [
        (due.id, true),
        (due.partition_key, true),
        (due.state, false),
        (due.workflow_type, false),
    ] {
        if component.is_empty()
            || (!typed && component.len() > limits.inline_bytes())
            || (typed && component.len() > MAX_RECORD_ID_BYTES)
        {
            return Err(ClaimPreflightError::Apply(ApplyError::InvalidAuthority));
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
    let next = u64::try_from(names)
        .ok()
        .and_then(|n| n.checked_mul(256))
        .and_then(|n| n.checked_add(1_024))
        .and_then(|n| g.checked_add(n))
        .ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
    if next > work.max_work_bytes() {
        return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
            bytes: next,
            maximum: work.max_work_bytes(),
        }));
    }
    *g = next;
    Ok(())
}

pub(super) fn guard_scan(
    scanned_count: &mut usize,
    g: &mut u64,
    work: ClaimWorkLimits,
) -> Result<(), ClaimPreflightError> {
    let scanned = scanned_count
        .checked_add(1)
        .ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
    let next = g.checked_add(1_024).ok_or(ClaimPreflightError::Business(
        ClaimRejection::LengthOverflow,
    ))?;
    if next > work.max_work_bytes() {
        return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
            bytes: next,
            maximum: work.max_work_bytes(),
        }));
    }
    *scanned_count = scanned;
    *g = next;
    Ok(())
}
