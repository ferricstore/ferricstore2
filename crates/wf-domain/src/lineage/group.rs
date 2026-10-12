//! Borrowed group validation and scalar terminal algebra.

use super::{ChildFailure, ChildGroup, ChildResolution, ChildStatus, ChildSummary, ChildWait};
use crate::workflow::active::{ActiveValueError, value_ref_bytes};
use crate::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_types::identity::RecordId;

pub(crate) fn validate_group(
    group: &ChildGroup,
    limits: AdmissionLimits,
) -> Result<usize, ActiveValueError> {
    let invalid = ActiveValueError::InvalidLineage;
    if group.success_state.is_empty()
        || group.failure_state.is_empty()
        || group.children.len() > limits.batch_items()
        || u64::try_from(group.children.len()).ok() != Some(group.summary.total)
    {
        return Err(invalid);
    }
    let mut bytes = 0_usize;
    for value in [
        &group.success_state,
        &group.failure_state,
        &group.request_hash,
    ] {
        if value.len() > limits.inline_bytes() {
            return Err(ActiveValueError::Admission(AdmissionError::ValueTooLarge {
                bytes: value.len(),
                maximum: limits.inline_bytes(),
            }));
        }
        bytes = bytes
            .checked_add(value.len())
            .ok_or(ActiveValueError::Admission(AdmissionError::LengthOverflow))?;
    }
    let mut summary = ChildSummary {
        total: group.summary.total,
        completed: 0,
        failed: 0,
        cancelled: 0,
    };
    for (id, member) in &group.children {
        let mut n =
            id.as_bytes().len() + member.partition.as_ref().map_or(0, |p| p.as_bytes().len());
        if (member.status == ChildStatus::Running) != member.result.is_none()
            || member
                .result
                .as_ref()
                .is_some_and(|result| result.status != member.status)
        {
            return Err(ActiveValueError::InvalidLineage);
        }
        if let Some(result) = &member.result {
            for reference in [result.result_ref.as_ref(), result.error_ref.as_ref()]
                .into_iter()
                .flatten()
            {
                n = n
                    .checked_add(value_ref_bytes(reference, limits)?)
                    .ok_or(ActiveValueError::Admission(AdmissionError::LengthOverflow))?;
            }
        }
        bytes = bytes
            .checked_add(n)
            .ok_or(ActiveValueError::Admission(AdmissionError::LengthOverflow))?;
        summary = count_terminal(summary, member.status).ok_or(ActiveValueError::InvalidLineage)?;
    }
    if summary != group.summary {
        return Err(ActiveValueError::InvalidLineage);
    }
    if bytes > limits.decision_bytes() {
        return Err(ActiveValueError::Admission(AdmissionError::DecisionBytes));
    }
    Ok(bytes)
}

pub(crate) fn count_terminal(
    mut summary: ChildSummary,
    status: ChildStatus,
) -> Option<ChildSummary> {
    let field = match status {
        ChildStatus::Running => return Some(summary),
        ChildStatus::Completed => &mut summary.completed,
        ChildStatus::Failed => &mut summary.failed,
        ChildStatus::Cancelled => &mut summary.cancelled,
    };
    *field = field.checked_add(1)?;
    Some(summary)
}

pub(crate) fn resolve_group(
    wait: ChildWait,
    failure: ChildFailure,
    summary: ChildSummary,
    prior: Option<ChildResolution>,
    status: ChildStatus,
) -> Option<ChildResolution> {
    if prior.is_some() {
        return prior;
    }
    if matches!(status, ChildStatus::Failed | ChildStatus::Cancelled)
        && failure == ChildFailure::FailParent
    {
        return Some(ChildResolution::Failure);
    }
    let terminal = summary
        .completed
        .checked_add(summary.failed)?
        .checked_add(summary.cancelled)?;
    if wait == ChildWait::Any {
        if summary.completed > 0 {
            Some(ChildResolution::Success)
        } else if terminal >= summary.total {
            Some(ChildResolution::Failure)
        } else {
            None
        }
    } else if terminal >= summary.total {
        Some(ChildResolution::Success)
    } else {
        None
    }
}

pub(crate) fn mark_cancelled(summary: ChildSummary, count: u64) -> Option<ChildSummary> {
    Some(ChildSummary {
        cancelled: summary.cancelled.checked_add(count)?,
        ..summary
    })
}

pub(crate) fn running_children(
    group: &ChildGroup,
) -> impl Iterator<Item = (&RecordId, &super::ChildMember)> {
    group
        .children
        .iter()
        .filter(|(_, member)| member.status == ChildStatus::Running)
}
