//! Fixed ETF metadata and borrowed policy payload fingerprinting.
use super::*;
pub(super) fn fingerprint(value: CreditPolicyInput<'_>) -> Result<PreparedDigest, E> {
    let _ = value;
    Err(E::UnsupportedVersion)
}
