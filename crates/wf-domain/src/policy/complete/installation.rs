//! Pure normalized Complete Replace preparation and borrowed generation decisions.
//!
//! The caller owns and threads the same schema charge through both operations.

use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};

use super::admission::charge_schema;
use super::validation::admit_complete;
use super::{
    CompletePolicyCandidate, CompletePolicyData, CompletePolicyEncodingError, CompletePolicyValue,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, prepare_complete_policy_value_v2,
    validate_stored_complete_policy, verify_complete_policy_value_v2,
};

/// Owned prepared replacement with the caller's optional generation guard.
pub struct PreparedCompletePolicyReplace {
    expected_generation: Option<u64>,
    value: CompletePolicyValue,
}

impl PreparedCompletePolicyReplace {
    /// Reads the caller's optional compare-and-set generation.
    #[must_use]
    pub const fn expected_generation(&self) -> Option<u64> {
        self.expected_generation
    }

    /// Borrows the original prepared complete policy value.
    #[must_use]
    pub const fn value(&self) -> &CompletePolicyValue {
        &self.value
    }
}

/// Pure replacement plan borrowing only the original prepared policy value.
pub struct CompletePolicyInstallPlan<'a> {
    expected_generation: u64,
    value: &'a CompletePolicyValue,
}

impl<'a> CompletePolicyInstallPlan<'a> {
    /// Reads the actual observed stored generation.
    #[must_use]
    pub const fn expected_generation(&self) -> u64 {
        self.expected_generation
    }

    /// Borrows the prepared policy with its original owner's lifetime.
    #[must_use]
    pub const fn value(&self) -> &'a CompletePolicyValue {
        self.value
    }
}

/// Prepares a normalized replacement using the actual stored and migration generations.
///
/// The original data moves into one canonical V2 value after current-policy and
/// caller-data admission. Omitted fields retain their replacement meaning.
///
/// # Errors
///
/// Returns profile, work, current-policy, caller-data, generation, or encoding
/// errors in the selected compare-and-set, admission, and late high-water order.
pub fn prepare_complete_policy_replace(
    existing: Option<&CompletePolicyValue>,
    migration_high_water: u64,
    expected_generation: Option<u64>,
    data: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<PreparedCompletePolicyReplace, CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    charge_schema(charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)?;
    let stored_generation = existing.map_or(0, CompletePolicyValue::generation);
    check_complete_replace_guard(stored_generation, expected_generation).map_err(|error| {
        CompletePolicyEncodingError::Schema(match error {
            PolicyError::InvalidAuthority => PolicySchemaError::InvalidAuthority,
            error => PolicySchemaError::Policy(error),
        })
    })?;
    if let Some(existing) = existing {
        validate_complete_replace_value(existing, &data.workflow_type, limits, charge)?;
    }
    admit_complete(&data, limits, charge, true).map_err(CompletePolicyEncodingError::Schema)?;
    if migration_high_water > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    let generation = stored_generation
        .max(migration_high_water)
        .checked_add(1)
        .filter(|generation| *generation <= MAX_POLICY_GENERATION)
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::GenerationExhausted),
        ))?;
    let candidate = CompletePolicyCandidate { generation, data };
    let value = prepare_complete_policy_value_v2(candidate, limits, charge)?;
    Ok(PreparedCompletePolicyReplace {
        expected_generation,
        value,
    })
}

/// Rechecks a prepared replacement and borrows its original policy value.
///
/// The plan records the actual observed stored generation. Its value lifetime
/// comes only from the prepared owner.
///
/// # Errors
///
/// Returns profile, work, current/prepared value, compare-and-set, late high-water,
/// or assigned-generation errors without refunding earlier admitted work.
pub fn decide_complete_policy_replace<'a>(
    existing: Option<&CompletePolicyValue>,
    migration_high_water: u64,
    workflow_type: &[u8],
    prepared: &'a PreparedCompletePolicyReplace,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyInstallPlan<'a>, CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    charge_schema(charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)?;
    let stored_generation = existing.map_or(0, CompletePolicyValue::generation);
    check_complete_replace_guard(stored_generation, prepared.expected_generation).map_err(
        |error| {
            CompletePolicyEncodingError::Schema(match error {
                PolicyError::InvalidAuthority => PolicySchemaError::InvalidAuthority,
                error => PolicySchemaError::Policy(error),
            })
        },
    )?;
    if let Some(existing) = existing {
        validate_complete_replace_value(existing, workflow_type, limits, charge)?;
    }
    validate_complete_replace_value(&prepared.value, workflow_type, limits, charge)?;
    if migration_high_water > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    let generation = stored_generation
        .max(migration_high_water)
        .checked_add(1)
        .filter(|generation| *generation <= MAX_POLICY_GENERATION)
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::GenerationExhausted),
        ))?;
    if prepared.value.generation() != generation {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::PreparedMismatch),
        ));
    }
    Ok(CompletePolicyInstallPlan {
        expected_generation: stored_generation,
        value: &prepared.value,
    })
}

/// Checks the actual stored scalar before the optional caller generation guard.
fn check_complete_replace_guard(stored: u64, expected: Option<u64>) -> Result<(), PolicyError> {
    if stored > MAX_POLICY_GENERATION {
        return Err(PolicyError::InvalidAuthority);
    }
    match expected {
        Some(expected) if expected > MAX_POLICY_GENERATION => Err(PolicyError::InvalidGeneration),
        Some(expected) if expected != stored => Err(PolicyError::StaleGeneration),
        _ => Ok(()),
    }
}

/// Pays stored content/type validation and V2 correspondence on the same account.
fn validate_complete_replace_value(
    value: &CompletePolicyValue,
    workflow_type: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyEncodingError> {
    validate_stored_complete_policy(value, workflow_type, limits, charge)
        .map_err(CompletePolicyEncodingError::Schema)?;
    verify_complete_policy_value_v2(value, limits, charge)
}
