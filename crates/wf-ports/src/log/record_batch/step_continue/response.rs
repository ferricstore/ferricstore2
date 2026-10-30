//! Explicit intent10/response9 data grammar, without publication authority.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::IndexedDecision;
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::log::{CodecError, writer::Writer};

mod scan;
mod write;

/// Encodes actual Step decision data without publication or retained authority.
/// # Errors
/// Rejects unsupported plan shape, invalid postimages, field caps and reservation.
pub fn encode_step_continue_response(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    let length = measure(decision, position, time, limits)?;
    encode_measured(decision, position, time, limits, length)
}

pub(super) fn measure(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<usize, CodecError> {
    let mut measured = Writer::measure(limits.decision_bytes());
    write::write(decision, position, time, &mut measured, limits)?;
    Ok(measured.len())
}

pub(super) fn encode_measured(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
    length: usize,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    let mut output = Writer::allocate(length, limits.decision_bytes())?;
    let status = write::write(decision, position, time, &mut output, limits)?;
    if output.len() != length {
        return Err(CodecError::Malformed);
    }
    Ok((output.finish()?, status))
}

/// Scans the complete selected intent10/response9 reply and original context.
/// # Errors
/// Rejects unsupported association, context mismatch, malformed bytes and caps.
pub fn step_continue_retained_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 10 || response_version != 9 {
        return Err(CodecError::UnsupportedVersion);
    }
    scan::summary(response, position, time, limits)
}

/// Compares complete reply bytes with the actual indexed Step decision.
/// # Errors
/// Rejects malformed/capped original bytes; valid alternative content is false.
pub fn step_continue_response_matches(
    decision: &IndexedDecision<'_>,
    response: &[u8],
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<bool, CodecError> {
    scan::summary(response, position, time, limits)?;
    let mut measured = Writer::measure(limits.decision_bytes());
    match write::write(decision, position, time, &mut measured, limits) {
        Ok(_) => {}
        Err(CodecError::Malformed) => return Ok(false),
        Err(error) => return Err(error),
    }
    if measured.len() != response.len() {
        return Ok(false);
    }
    let mut verified = Writer::verify(response, limits.decision_bytes())?;
    match write::write(decision, position, time, &mut verified, limits)
        .and_then(|_| verified.finish_verification())
    {
        Ok(()) => Ok(true),
        Err(CodecError::Malformed) => Ok(false),
        Err(error) => Err(error),
    }
}
