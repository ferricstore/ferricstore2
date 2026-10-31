//! One source-format batch with fallible exact per-ID owners.
use super::*;
pub(super) fn generate(
    amount: u64,
    spend: CreditSpendPreparation,
    maximum_owned: usize,
) -> Result<Vec<Vec<u8>>, E> {
    let _ = (amount, spend, maximum_owned);
    Err(E::UnsupportedVersion)
}
