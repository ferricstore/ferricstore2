//! Borrowed donor measurement, independent of encoded-content correspondence.

use super::{PolicySchemaCharge, PolicySchemaLimits, complete_metrics};
use crate::policy::complete::{CompletePolicyValue, PolicySchemaError};

/// Measures typed nodes, typed variable bytes and the independent encoded owner.
///
/// This performs no allocation and proves no correspondence between the owners.
/// The caller must retain this same account through subsequent verification.
/// # Errors
/// Refuses a different account profile, schema work or checked size overflow.
pub fn complete_policy_value_owned_bytes(
    value: &CompletePolicyValue,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, PolicySchemaError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch);
    }
    let (rows, variable, _) = complete_metrics(value.data(), limits, charge)?;
    let bytes = rows
        .checked_mul(512)
        .and_then(|n| n.checked_add(variable))
        .and_then(|n| n.checked_add(2_048))
        .and_then(|n| n.checked_add(u64::try_from(value.encoded().len()).ok()?))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    usize::try_from(bytes).map_err(|_| PolicySchemaError::LengthOverflow)
}
