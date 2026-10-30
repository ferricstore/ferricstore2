//! Explicit Step retained data; generic store profiles remain closed.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_types::identity::RequestId;

use super::{ScannedRetainedOutcome, StateValueError};

/// Encodes one explicitly selected Step receipt under actual captured field caps.
/// # Errors
/// Currently refuses every input until the selected tests-first body exists.
pub fn encode_step_continue_outcome(
    _outcome: &RetainedOutcome,
    _maximum_wire_bytes: usize,
    _admission: AdmissionLimits,
) -> Result<Vec<u8>, StateValueError> {
    Err(StateValueError::UnsupportedVersion)
}

/// Scans the entire selected receipt before admitting any materialized owner.
/// # Errors
/// Currently refuses every input until the selected tests-first body exists.
pub fn scan_step_continue_outcome<'input>(
    _bytes: &'input [u8],
    _expected_scope: &RequestScope,
    _expected_request: &RequestId,
    _admission: AdmissionLimits,
    _maximum_wire_bytes: usize,
) -> Result<ScannedRetainedOutcome<'input>, StateValueError> {
    Err(StateValueError::UnsupportedVersion)
}
