//! Explicit singleton intent10 preparation; no apply or durability authority.

use crate::consensus::PreparedRequest;
use crate::log::{CodecLimits, DecodedRequest, PreparedBytes, digest, writer::Writer};
use wf_domain::workflow::request::ClientIntent;

use super::{RecordBatchApplyError, admit, admit_decoded_request, sum};

mod input;
mod response;
mod wire;

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
    let input = input::borrow(&request)?;
    let mut measured = Writer::measure(limits.admission.decision_bytes());
    wire::write(
        &input,
        ClientIntent::step_continue([0; 32]),
        &mut measured,
        limits,
    )?;
    admit(
        sum(&[input::variable_bytes(&input)?, 2048, measured.len()])?,
        workspace_limit,
    )?;
    let intent = input::derive(&input)?;
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
    let input = wire::scan(encoded, limits)?;
    admit_decoded_request(encoded.len(), 1, limits, workspace_limit)?;
    let intent = input::derive(&input)?;
    if input.intent != intent {
        return Err(RecordBatchApplyError::IntentMismatch);
    }
    Ok(wire::materialize(&input, encoded, intent, limits)?)
}
