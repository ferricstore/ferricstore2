//! Owned source FSV2/ETF value compatibility, independent of policy envelopes.

mod model;

pub use model::{
    OriginalFlowValueError, OriginalValueContext, OriginalValueDocument,
    OriginalValueFunctionIdentity, OriginalValueLimits, OriginalValueNode,
};

/// Decode stored source value bytes into an owned typed document.
///
/// This new callable baseline rejects every input. Parsing, context checks and
/// operational limit enforcement are not implemented in this R0 increment.
/// Foreign identities will be data, without a promise to execute BEAM resources.
///
/// # Errors
///
/// Always returns [`OriginalFlowValueError::InvalidFlowValue`] in this baseline.
pub fn decode_original_flow_value(
    _encoded: Vec<u8>,
    _context: &OriginalValueContext<'_>,
    _limits: OriginalValueLimits,
) -> Result<OriginalValueDocument, OriginalFlowValueError> {
    Err(OriginalFlowValueError::InvalidFlowValue)
}
