//! Bounded borrowed semantics for the original stored-policy deprecated key.

use super::PolicySchemaLimits;
use super::admission::charge_schema;
use super::{CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError};

mod map;

/// Borrowed key classes used by the original stored-policy map predicate.
///
/// These descriptors supply no ETF decoding, atom-safety or source-map identity
/// proof. Distinct opaque source keys may each be represented by `Other`.
#[derive(Clone, Copy, Debug)]
pub enum OriginalPolicyMapKey<'input> {
    /// An atom's borrowed spelling; atom and binary keys remain distinct.
    Atom(&'input str),
    /// A borrowed binary key, without UTF-8 or normalization requirements.
    Binary(&'input [u8]),
    /// A key whose original term identity and payload are not represented.
    Other,
}

/// Borrowed value classes used by the original stored-policy map predicate.
///
/// A supplied map view is mathematical input, not a parsed or validated source
/// map. `Other` represents a nonmap class without preserving its term payload.
#[derive(Clone, Copy, Debug)]
pub enum OriginalPolicyMapValue<'input> {
    /// Borrowed entries in the supplied traversal order.
    Map(&'input [(OriginalPolicyMapKey<'input>, Self)]),
    /// A nonmap value, including source values whose payload is not represented.
    Other,
}

/// Checks the reached original `max_attempts` keys without business admission.
///
/// The preflight bounds visible descriptors to depth 64, 4096 occurrences and
/// 262144 represented key bytes, then rejects duplicate queried atom/binary key
/// identities in each map. It does not prove arbitrary source-map uniqueness or
/// account for raw terms hidden by `Other`. Every reached operation freshly uses
/// the received account. Input owners remain borrowed and unchanged.
///
/// Root atom keys shadow binary keys even when nonmap. State retry selection
/// instead falls through a nonmap atom value to a binary map. Reached deprecated
/// key presence is sufficient regardless of its value. This predicate supplies
/// no envelope, generation, current-policy, raw ETF or retained-gate authority.
///
/// # Errors
/// Returns operational profile, work, checked length, visible descriptor/byte
/// cap or duplicate queried-key errors, retaining earlier successful debits.
pub fn original_complete_policy_uses_max_attempts(
    policy: &[(OriginalPolicyMapKey<'_>, OriginalPolicyMapValue<'_>)],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    map::preflight(policy, charge)?;
    charge_schema(charge, 0, 128).map_err(Error::Schema)?;
    if let Some(OriginalPolicyMapValue::Map(retry)) =
        map::select_top_field(policy, b"retry", charge)?
        && map::contains_old_key(retry, charge)?
    {
        return Ok(true);
    }
    if let Some(OriginalPolicyMapValue::Map(states)) =
        map::select_top_field(policy, b"states", charge)?
    {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        for &(_, value) in states {
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            if let OriginalPolicyMapValue::Map(state) = value
                && let Some(retry) = map::select_state_retry_map(state, charge)?
                && map::contains_old_key(retry, charge)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
