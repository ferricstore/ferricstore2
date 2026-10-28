//! Explicit singleton intent10 preparation; no apply or durability authority.

use crate::consensus::PreparedRequest;
use crate::log::{CodecError, CodecLimits, DecodedRequest, PreparedBytes};

use super::RecordBatchApplyError;

/// Derive caller intent and prepare complete singleton Step bytes.
///
/// This data-only codec does not select a response, publish a workflow or open
/// legacy profiles. The supplied mathematical intent is not trusted.
///
/// # Errors
/// Rejects unsupported shape/profile, field limits, workspace and reservations.
pub fn encode_step_continue(
    _request: PreparedRequest,
    _limits: CodecLimits,
    _workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    Err(CodecError::UnsupportedTag.into())
}

/// Verify complete bounded bytes before materializing their owned fields.
///
/// No Fresh gate, persistence capability or recovery dispatch is supplied.
///
/// # Errors
/// Rejects malformed/unsupported input, intent mismatch and operational limits.
pub fn prepare_step_continue(
    _encoded: &[u8],
    _limits: CodecLimits,
    _workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    Err(CodecError::UnsupportedTag.into())
}
