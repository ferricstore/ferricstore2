//! Versioned workflow value command boundary; unsupported until implemented.

use crate::consensus::{
    PreparedWorkflowValueBatch, WorkflowValueDefaults, WorkflowValueItemInput,
    WorkflowValuePolicyWitness,
};

use super::{CodecError, CodecLimits, DecodedRequest, PreparedBytes};

/// Borrowed value metadata beside one decoded operation; no apply authority.
#[derive(Clone, Copy)]
pub struct WorkflowValueMetadata<'a> {
    /// Contained reducer profile.
    pub profile: u8,
    /// Explicit presence and value of the captured-reference marker.
    pub policy_references_captured: Option<bool>,
    /// Recorded lifecycle configuration and its content identity.
    pub defaults: WorkflowValueDefaults,
    /// Exact backing witnesses borrowed from the decoded owner.
    pub policies: &'a [WorkflowValuePolicyWitness],
    /// Ordered source presence and target inputs borrowed from that owner.
    pub inputs: &'a [WorkflowValueItemInput],
}

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

/// Prepares one bounded owner and reports its continuous logical work spend.
///
/// # Errors
/// Refuses unsupported workflow value command profiles.
pub fn prepare_workflow_value_batch(
    _encoded: &[u8],
    _limits: CodecLimits,
    _maximum_owned: usize,
    _maximum_work: usize,
) -> Result<(DecodedRequest<'_>, usize), CodecError> {
    Err(CodecError::UnsupportedTag)
}

impl DecodedRequest<'_> {
    /// Borrows captured value inputs beside the single decoded operation owner.
    #[must_use]
    pub fn workflow_value_metadata(&self) -> Option<WorkflowValueMetadata<'_>> {
        None
    }
}
