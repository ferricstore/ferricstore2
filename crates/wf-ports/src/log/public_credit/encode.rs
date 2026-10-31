//! Canonical caller bytes and complete original public prepared frame.
use super::*;
pub(super) fn intent(call: NormalizedCreditCall<'_>) -> Result<ClientIntent, E> {
    let _ = call;
    Err(E::UnsupportedVersion)
}
pub(super) fn request(
    scope: &RequestScope,
    id: &RequestId,
    call: NormalizedCreditCall<'_>,
    stamp: CommittedTime,
    spend: Option<CreditSpendPreparation>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    let _ = (scope, id, call, stamp, spend, limits, maximum_owned);
    Err(E::UnsupportedVersion)
}
