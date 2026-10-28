//! Explicit legacy fallback, independent of strict history-value decoding.

use crate::value::original_flow::{
    OriginalFlowValueError as E, OriginalValueContext as C, OriginalValueDocument as D,
    OriginalValueLimits as L,
};

/// Decode with the pinned legacy fallback and exact stored user-byte size.
///
/// Invalid typed ETF becomes a binary over its original suffix. The returned
/// size excludes a recognized five-byte FSV2 wrapper and includes ETF version.
/// This API grants neither storage nor runtime authority.
///
/// # Errors
///
/// This rejecting TDD facade is not implemented. Compatibility/profile and
/// operational resource failures must remain errors rather than raw fallback.
pub fn decode_original_flow_value_compat(
    _encoded: Vec<u8>,
    _context: &C<'_>,
    _limits: L,
) -> Result<(D, usize), E> {
    Err(E::InvalidFlowValue)
}
