//! Counted scalar rows measured and checked directly against ordered owned results.

use wf_domain::workflow::admission::MAX_BATCH_ITEMS;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexRejection};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};

use super::super::super::CodecError;
use super::{domain, push_rejection, record_batch_retained_summary, rejection};

pub(in crate::log::record_batch) fn measure(
    decision: &IndependentIndexedDecision<'_>,
) -> Result<usize, CodecError> {
    let plan = match decision {
        IndependentIndexedDecision::Accepted(plan) => plan,
        IndependentIndexedDecision::Rejected(reason) => return rejection_size(*reason),
    };
    let count = plan.results().len();
    if count == 0 || count > MAX_BATCH_ITEMS {
        return Err(CodecError::ItemLimit);
    }
    plan.results().iter().try_fold(5_usize, |size, result| {
        let row = match result {
            Ok(_) => 1,
            Err(reason) => {
                item_reason(*reason)?;
                rejection_size(*reason)?
            }
        };
        size.checked_add(row).ok_or(CodecError::LengthOverflow)
    })
}

fn rejection_size(reason: IndexRejection) -> Result<usize, CodecError> {
    Ok(match reason {
        IndexRejection::Batch(BatchRejection::Domain(reason)) => {
            3 + usize::from(domain(reason).1.is_some())
        }
        IndexRejection::Batch(BatchRejection::Admission(
            wf_domain::workflow::admission::AdmissionError::ValueTooLarge { bytes, maximum },
        )) => {
            u64::try_from(bytes).map_err(|_| CodecError::LengthOverflow)?;
            u64::try_from(maximum).map_err(|_| CodecError::LengthOverflow)?;
            19
        }
        _ => 3,
    })
}

pub(in crate::log::record_batch) fn encode(
    decision: &IndependentIndexedDecision<'_>,
    size: usize,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    if measure(decision)? != size {
        return Err(CodecError::Malformed);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| CodecError::Allocation)?;
    let status = match decision {
        IndependentIndexedDecision::Accepted(plan) => {
            bytes.push(2);
            bytes.extend_from_slice(
                &u32::try_from(plan.results().len())
                    .map_err(|_| CodecError::LengthOverflow)?
                    .to_be_bytes(),
            );
            for result in plan.results() {
                match result {
                    Ok(_) => bytes.push(0),
                    Err(reason) => push_rejection(&mut bytes, *reason)?,
                }
            }
            RetainedStatus::Accepted
        }
        IndependentIndexedDecision::Rejected(reason) => {
            push_rejection(&mut bytes, *reason)?;
            RetainedStatus::IndexedRejected(*reason)
        }
    };
    Ok((bytes, status))
}

pub(super) fn summary(response: &[u8]) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    let count = count(response)?;
    let mut rest = &response[5..];
    for _ in 0..count {
        let (_, size) = row(rest)?;
        rest = rest.get(size..).ok_or(CodecError::Malformed)?;
    }
    if !rest.is_empty() {
        return Err(CodecError::Malformed);
    }
    Ok((RetainedStatus::Accepted, Some(count)))
}

fn count(response: &[u8]) -> Result<usize, CodecError> {
    if response.first() != Some(&2) {
        return Err(CodecError::UnsupportedTag);
    }
    let count = usize::try_from(u32::from_be_bytes(
        response
            .get(1..5)
            .ok_or(CodecError::Malformed)?
            .try_into()
            .map_err(|_| CodecError::Malformed)?,
    ))
    .map_err(|_| CodecError::LengthOverflow)?;
    if count == 0 || count > MAX_BATCH_ITEMS {
        return Err(CodecError::ItemLimit);
    }
    Ok(count)
}

fn row(response: &[u8]) -> Result<(Option<IndexRejection>, usize), CodecError> {
    match response.first().copied().ok_or(CodecError::Malformed)? {
        0 => Ok((None, 1)),
        1 => {
            let (reason, size) = rejection(response)?;
            item_reason(reason)?;
            Ok((Some(reason), size))
        }
        _ => Err(CodecError::UnsupportedTag),
    }
}

fn item_reason(reason: IndexRejection) -> Result<(), CodecError> {
    match reason {
        IndexRejection::Batch(BatchRejection::Domain(_))
        | IndexRejection::InvalidPriority
        | IndexRejection::FifoPriority => Ok(()),
        _ => Err(CodecError::UnsupportedTag),
    }
}

/// Compares count and every closed scalar row against the actual ordered decision.
/// Structurally valid mismatches return false; malformed input always refuses.
/// # Errors
/// Rejects unknown/forbidden rows, invalid counts and incomplete/trailing bytes.
pub fn independent_record_batch_response_matches(
    decision: &IndependentIndexedDecision<'_>,
    response: &[u8],
) -> Result<bool, CodecError> {
    if response.first() != Some(&2) {
        let (status, count) = record_batch_retained_summary(
            ClientIntent::independent_record_batch([0; 32]),
            1,
            response,
        )?;
        return Ok(count.is_none()
            && matches!(decision,
            IndependentIndexedDecision::Rejected(reason) if status == RetainedStatus::IndexedRejected(*reason)));
    }
    let count = count(response)?;
    let mut rest = &response[5..];
    let mut matches = matches!(decision, IndependentIndexedDecision::Accepted(plan) if plan.results().len() == count);
    for index in 0..count {
        let (actual, size) = row(rest)?;
        if let IndependentIndexedDecision::Accepted(plan) = decision {
            matches &= plan
                .results()
                .get(index)
                .is_some_and(|expected| match expected {
                    Ok(_) => actual.is_none(),
                    Err(reason) => actual == Some(*reason),
                });
        }
        rest = rest.get(size..).ok_or(CodecError::Malformed)?;
    }
    if !rest.is_empty() {
        return Err(CodecError::Malformed);
    }
    Ok(matches)
}
