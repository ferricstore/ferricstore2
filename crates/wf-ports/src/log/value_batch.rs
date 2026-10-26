//! Versioned workflow value command boundary; unsupported until implemented.

use crate::consensus::PreparedWorkflowValueBatch;

use super::{CodecError, CodecLimits, PreparedBytes};

/// Encodes an admitted prepared workflow value batch.
///
/// # Errors
/// Refuses unsupported workflow value command profiles.
pub fn encode_workflow_value_batch(
    _batch: &PreparedWorkflowValueBatch,
    _limits: CodecLimits,
) -> Result<PreparedBytes, CodecError> {
    Err(CodecError::UnsupportedTag)
}

/// Decodes a bounded workflow value command with explicit input presence.
///
/// # Errors
/// Refuses unsupported workflow value command profiles.
pub fn decode_workflow_value_batch(
    _encoded: &[u8],
    _limits: CodecLimits,
) -> Result<PreparedWorkflowValueBatch, CodecError> {
    Err(CodecError::UnsupportedTag)
}
