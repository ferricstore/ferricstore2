//! Borrow actual normalized index fields while the verified full donor stays alive.
use super::{
    CompletePolicyEncodingError as E, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    StoredPolicyValue, original,
};
use crate::policy::complete::{
    charge_schema, complete_policy_value_owned_bytes, verify_complete_policy_value_v2,
};

/// Ordinary borrowed configuration and shared-account observations, never authority.
pub struct BorrowedPolicyIndexedFields<'a> {
    /// At most three normalized actual attribute names.
    pub attributes: [Option<&'a [u8]>; 3],
    /// Actual normalized state metadata name, or its known absence.
    pub state_metadata: Option<&'a [u8]>,
    /// Zero absent, one Complete, two Original backing.
    pub profile: u8,
    /// Actual current generation; absent policy remains distinct.
    pub generation: Option<u64>,
    /// Logical source-owner size, excluding its enclosing union descriptor.
    pub owned_bytes: usize,
    /// Current shared schema total, including scratch liability.
    pub work_units: u64,
    /// Current shared schema peak scratch liability.
    pub temporary_bytes: u64,
}

/// Inspect actual full backing and consume borrowed normalized configuration.
/// The result cannot borrow a temporary Original index through this callback.
/// # Errors
/// Preserve type, schema, profile, resource and original normalization failures.
pub fn with_stored_policy_indexed_fields<T>(
    policy: Option<&StoredPolicyValue>,
    kind: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    consume: impl FnOnce(BorrowedPolicyIndexedFields<'_>) -> T,
) -> Result<T, E> {
    if charge.profile() != limits {
        return Err(E::Schema(PolicySchemaError::ProfileMismatch));
    }
    let amount = u64::try_from(kind.len())
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(E::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, amount, 0).map_err(E::Schema)?;
    if kind.is_empty() {
        return Err(E::Schema(PolicySchemaError::InvalidAuthority));
    }
    let mut names = [None; 3];
    match policy {
        None => Ok(consume(view(names, None, (0, None, 0), charge))),
        Some(StoredPolicyValue::Complete(value)) => {
            verify_complete_policy_value_v2(value, limits, charge)?;
            if value.data().workflow_type != kind {
                return Err(E::Schema(PolicySchemaError::InvalidAuthority));
            }
            let owned =
                complete_policy_value_owned_bytes(value, limits, charge).map_err(E::Schema)?;
            for (slot, name) in names.iter_mut().zip(
                value
                    .data()
                    .indexed_attributes
                    .as_deref()
                    .unwrap_or_default(),
            ) {
                *slot = Some(name.as_slice());
            }
            Ok(consume(view(
                names,
                value.data().indexed_state_meta.as_deref(),
                (1, Some(value.generation()), owned),
                charge,
            )))
        }
        Some(StoredPolicyValue::Original(raw)) => {
            let index = original::read(raw, kind, limits, charge)?;
            original::attributes(Some(&index), &mut names, charge)?;
            let state = original::metadata(Some(&index), charge)?;
            Ok(consume(view(
                names,
                state,
                (2, Some(index.entry().generation()), raw.len()),
                charge,
            )))
        }
    }
}
fn view<'a>(
    attributes: [Option<&'a [u8]>; 3],
    state_metadata: Option<&'a [u8]>,
    (profile, generation, owned_bytes): (u8, Option<u64>, usize),
    charge: &PolicySchemaCharge,
) -> BorrowedPolicyIndexedFields<'a> {
    BorrowedPolicyIndexedFields {
        attributes,
        state_metadata,
        profile,
        generation,
        owned_bytes,
        work_units: charge.total(),
        temporary_bytes: charge.p(),
    }
}
