//! Closed public framing followed by bounded private operand materialization.
use super::*;
pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCreditRequest<'_>, E> {
    let _ = (encoded, limits, maximum_owned);
    Err(E::UnsupportedVersion)
}
pub(super) fn recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    let _ = (encoded, limits);
    Err(E::UnsupportedVersion)
}
