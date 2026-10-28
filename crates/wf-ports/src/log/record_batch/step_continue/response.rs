//! Selected Step reply interface; rejecting until its genuine tests are saved.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::IndexedDecision;
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::log::CodecError;

/// Encodes actual Step decision data without publication or retained authority.
/// # Errors
/// Currently refuses every reply until the selected bounded grammar is implemented.
pub fn encode_step_continue_response(
    _decision: &IndexedDecision<'_>,
    _position: LogIdentity,
    _time: TimestampMillis,
    _limits: AdmissionLimits,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    Err(CodecError::UnsupportedTag)
}

/// Scans the complete selected intent10/response9 reply and original context.
/// # Errors
/// Currently refuses every reply until the selected bounded grammar is implemented.
pub fn step_continue_retained_summary(
    _intent: ClientIntent,
    _response_version: u16,
    _response: &[u8],
    _position: LogIdentity,
    _time: TimestampMillis,
    _limits: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    Err(CodecError::UnsupportedTag)
}

/// Compares complete reply bytes with the actual indexed Step decision.
/// # Errors
/// Currently refuses every reply until the selected bounded grammar is implemented.
pub fn step_continue_response_matches(
    _decision: &IndexedDecision<'_>,
    _response: &[u8],
    _position: LogIdentity,
    _time: TimestampMillis,
    _limits: AdmissionLimits,
) -> Result<bool, CodecError> {
    Err(CodecError::UnsupportedTag)
}
