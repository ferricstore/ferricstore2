//! Explicit singleton binary/stored Step preparation; no durability authority.

use crate::consensus::PreparedRequest;
use crate::log::{CodecLimits, DecodedRequest, PreparedBytes, digest, writer::Writer};
use wf_domain::workflow::request::ClientIntent;

use super::{RecordBatchApplyError, admit, admit_decoded_request, sum};

mod fresh;
mod input;
mod response;
mod wire;

/// Prepares explicit stored-payload Step bytes.
/// # Errors
/// Refuses unsupported selection or malformed and over-limit input.
pub fn encode_stored_step_continue(
    request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    encode_profile(request, limits, workspace_limit, 11)
}

/// Validates an explicit stored-payload Step before ownership.
/// # Errors
/// Refuses unsupported selection or malformed and over-limit input.
pub fn prepare_stored_step_continue(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    prepare_profile(encoded, limits, workspace_limit, 11)
}

pub use fresh::{decide_fresh_step_continue_touched, inspect_step_continue_point};

pub use response::{
    encode_step_continue_response, step_continue_response_matches, step_continue_retained_summary,
};

/// Derive caller intent and prepare complete singleton Step bytes.
///
/// This data-only codec does not select a response, publish a workflow or open
/// legacy profiles. The supplied mathematical intent is not trusted.
///
/// # Errors
/// Rejects unsupported shape/profile, field limits, workspace and reservations.
pub fn encode_step_continue(
    request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    encode_profile(request, limits, workspace_limit, 10)
}

fn encode_profile(
    request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
    version: u16,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    let input = input::borrow(&request)?;
    let mut measured = Writer::measure(limits.admission.decision_bytes());
    let identity = if version == 11 {
        ClientIntent::stored_step_continue([0; 32])
    } else {
        ClientIntent::step_continue([0; 32])
    };
    wire::write(&input, identity, &mut measured, limits)?;
    admit(
        sum(&[input::variable_bytes(&input)?, 2048, measured.len()])?,
        workspace_limit,
    )?;
    let intent = input::derive(&input, version)?;
    let mut output = Writer::allocate(measured.len(), limits.admission.decision_bytes())?;
    wire::write(&input, intent, &mut output, limits)?;
    let bytes = output.finish()?;
    drop(request);
    Ok(PreparedBytes {
        digest: digest(&bytes),
        bytes,
    })
}

/// Verify complete bounded bytes before materializing their owned fields.
///
/// No Fresh gate, persistence capability or recovery dispatch is supplied.
///
/// # Errors
/// Rejects malformed/unsupported input, intent mismatch and operational limits.
pub fn prepare_step_continue(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    prepare_profile(encoded, limits, workspace_limit, 10)
}

fn prepare_profile(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
    version: u16,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    let input = wire::scan(encoded, limits, version)?;
    admit_decoded_request(encoded.len(), 1, limits, workspace_limit)?;
    let intent = input::derive(&input, version)?;
    if input.intent != intent {
        return Err(RecordBatchApplyError::IntentMismatch);
    }
    Ok(wire::materialize(&input, encoded, intent, limits)?)
}
