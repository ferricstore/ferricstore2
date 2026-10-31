use super::{CreditCallKind, CreditCallerError, CreditOptionValue, NormalizedCreditCall};
/// Captures bounded source keyword options without reading owner state or time.
/// # Errors
/// Refuses invalid options, bounds, ID/configuration fields and raw-byte limits.
pub fn normalize_credit_call<'a>(
    kind: CreditCallKind,
    scope: &'a [u8],
    count: u64,
    options: &[(&str, CreditOptionValue<'a>)],
    maximum_raw_bytes: usize,
) -> Result<NormalizedCreditCall<'a>, CreditCallerError> {
    let _ = (kind, scope, count, options, maximum_raw_bytes);
    Err(CreditCallerError::InvalidOptions)
}
