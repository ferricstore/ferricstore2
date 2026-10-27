//! Logical value preparation, distinct from already-stored byte ingress.

mod model;

pub use model::{
    OriginalValueEncodeError, OriginalValueEncodeLimits, OriginalValueInput,
    PreparedOriginalFlowValue,
};

/// Prepare a logical value as complete source-compatible stored bytes.
///
/// The input borrows a flat postorder arena rather than encoded ETF. Successful
/// output owns its bytes and carries no storage, runtime or admission authority.
///
/// # Errors
///
/// Invalid logical data, invalid profiles, resource refusal and actual failed
/// reservations remain distinct. This rejecting TDD facade is not implemented.
pub fn prepare_original_flow_value(
    _input: OriginalValueInput<'_>,
    _limits: OriginalValueEncodeLimits,
) -> Result<PreparedOriginalFlowValue, OriginalValueEncodeError> {
    Err(OriginalValueEncodeError::InvalidInput)
}
