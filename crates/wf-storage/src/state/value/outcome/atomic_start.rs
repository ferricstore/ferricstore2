//! Borrowed atomic-start response admission within the shared retained framing.

use wf_domain::workflow::admission::{AdmissionLimits, MAX_BATCH_ITEMS, MAX_INLINE_BYTES};
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_ports::log::atomic_start_retained_summary;
use wf_types::version::{LogIdentity, TimestampMillis};

use super::{StateValueError, claim_error};

pub(super) fn wire_status(
    outcome: &RetainedOutcome,
    maximum_wire_bytes: usize,
) -> Result<RetainedStatus, StateValueError> {
    // Wire ceilings admit representation only; readers repeat captured limits.
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, maximum_wire_bytes, MAX_BATCH_ITEMS)
        .map_err(|_| StateValueError::InvalidValue)?;
    scan(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_position(),
        outcome.original_time(),
        limits,
    )
    .map(|(status, _)| status)
}

pub(super) fn scan(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    original_position: LogIdentity,
    original_time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), StateValueError> {
    let (status, count) = atomic_start_retained_summary(
        intent,
        response_version,
        response,
        original_position,
        original_time,
        limits,
    )
    .map_err(claim_error)?;
    let expected = match status {
        RetainedStatus::Accepted => Some(1),
        RetainedStatus::IndexedRejected(_) => None,
        _ => return Err(StateValueError::InvalidValue),
    };
    if count != expected {
        return Err(StateValueError::InvalidValue);
    }
    Ok((status, count))
}
