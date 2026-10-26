//! Scalar public responses with disjoint whole and sole-item origins.

use wf_domain::value::DomainError;
use wf_domain::workflow::admission::AdmissionError;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexRejection};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};

use super::super::{
    CodecError,
    record_batch::{push_rejection, rejection},
};

fn allowed(root: u8, reason: IndexRejection) -> bool {
    match root {
        1 => matches!(
            reason,
            IndexRejection::Batch(BatchRejection::Admission(_) | BatchRejection::CrossGroup)
        ),
        2 => matches!(
            reason,
            IndexRejection::Batch(BatchRejection::Domain(_))
                | IndexRejection::InvalidPriority
                | IndexRejection::FifoPriority
        ),
        _ => false,
    }
}

fn expected(
    decision: &IndependentIndexedDecision<'_>,
) -> Result<(u8, Option<IndexRejection>), CodecError> {
    let (root, reason) = match decision {
        IndependentIndexedDecision::Rejected(reason) => (1, Some(*reason)),
        IndependentIndexedDecision::Accepted(plan) => {
            if plan.results().len() != 1 {
                return Err(CodecError::ItemLimit);
            }
            match &plan.results()[0] {
                Ok(_) => (0, None),
                Err(reason) => (2, Some(*reason)),
            }
        }
    };
    if reason.is_some_and(|reason| !allowed(root, reason)) {
        return Err(CodecError::UnsupportedTag);
    }
    Ok((root, reason))
}

/// Validates the entire scalar profile without granting retained origin authority.
/// # Errors
/// Rejects crossed profiles, forbidden error origins, incomplete or trailing bytes.
pub fn public_mutation_retained_status(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
) -> Result<RetainedStatus, CodecError> {
    if intent.version() != 1 || response_version != 2 {
        return Err(CodecError::UnsupportedVersion);
    }
    body_status(response)
}

pub(in crate::log) fn body_status(response: &[u8]) -> Result<RetainedStatus, CodecError> {
    if response == [0] {
        return Ok(RetainedStatus::Accepted);
    }
    let root = *response.first().ok_or(CodecError::Malformed)?;
    if !matches!(root, 1 | 2) || response.len() > 19 {
        return Err(CodecError::UnsupportedTag);
    }
    let mut scalar = [0; 19];
    scalar[..response.len()].copy_from_slice(response);
    scalar[0] = 1;
    let (reason, size) = rejection(&scalar[..response.len()])?;
    if size != response.len() {
        return Err(CodecError::TrailingBytes);
    }
    if !allowed(root, reason) {
        return Err(CodecError::UnsupportedTag);
    }
    Ok(RetainedStatus::IndexedRejected(reason))
}

/// Compares scalar count, full diagnosis and encoded origin to the actual plan.
/// # Errors
/// Rejects malformed grammar or an unrepresentable mathematical decision.
pub fn public_mutation_response_matches(
    decision: &IndependentIndexedDecision<'_>,
    response: &[u8],
) -> Result<bool, CodecError> {
    let status = body_status(response)?;
    let (root, reason) = expected(decision)?;
    Ok(response.first() == Some(&root)
        && status == reason.map_or(RetainedStatus::Accepted, RetainedStatus::IndexedRejected))
}

pub(super) fn measure(decision: &IndependentIndexedDecision<'_>) -> Result<usize, CodecError> {
    let (_, reason) = expected(decision)?;
    Ok(match reason {
        None => 1,
        Some(IndexRejection::Batch(BatchRejection::Admission(AdmissionError::ValueTooLarge {
            ..
        }))) => 19,
        Some(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::EmptyValue(_)
            | DomainError::CounterOverflow(_)
            | DomainError::InvalidRetryPolicy(_),
        ))) => 4,
        Some(_) => 3,
    })
}

pub(super) fn encode(
    decision: &IndependentIndexedDecision<'_>,
    size: usize,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    if size != measure(decision)? {
        return Err(CodecError::Malformed);
    }
    let (root, reason) = expected(decision)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| CodecError::Allocation)?;
    if let Some(reason) = reason {
        push_rejection(&mut bytes, reason)?;
        bytes[0] = root;
    } else {
        bytes.push(0);
    }
    Ok((
        bytes,
        reason.map_or(RetainedStatus::Accepted, RetainedStatus::IndexedRejected),
    ))
}
