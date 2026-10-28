//! Bind an owned logical caller to one explicit stored-value frame.

mod model;

pub use model::{LogicalMutationFrameContext, LogicalMutationFrameLimits};

use wf_domain::workflow::caller::PreparedLogicalMutation;
use wf_domain::workflow::request::ClientIntent;

use crate::log::{CodecError, PreparedBytes};

/// Encode one logical caller as stored intent9, retaining its original evidence.
///
/// Context values are explicit observations, without authorization, placement,
/// submission or persistence authority. The owned caller is consumed.
///
/// # Errors
/// This rejecting TDD facade is not implemented. Metadata, resource, overflow
/// and reservation errors must retain their existing codec error lanes.
pub fn encode_logical_mutation_frame(
    _caller: PreparedLogicalMutation,
    _context: LogicalMutationFrameContext,
    _limits: LogicalMutationFrameLimits,
) -> Result<(PreparedBytes, ClientIntent), CodecError> {
    Err(CodecError::UnsupportedTag)
}
