//! Owned restoration and bounded range/current-value admission without copying.

use std::ops::Range;

use crate::policy::installation::{
    MAX_ENCODED_POLICY_BYTES, MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES, PolicyError,
};

use super::admission::charge_schema;
use super::validation::admit_complete;
use super::{
    CompletePolicyCandidate, CompletePolicyData, CompletePolicyValue, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};

/// Restores supplied normalized owners with an explicit internal generation.
///
/// This move-only constructor neither allocates a generation nor installs or
/// certifies encoded/source/namespace authority. Refusal drops the supplied owners;
/// bounded construction and destruction remain outer preparation obligations.
///
/// # Errors
/// Preserves profile/work diagnoses and typed generation/content refusals.
pub fn restore_complete_policy(
    generation: u64,
    data: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyCandidate, PolicySchemaError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch);
    }
    if generation == 0 || generation > MAX_POLICY_GENERATION {
        return Err(PolicySchemaError::Policy(PolicyError::InvalidGeneration));
    }
    admit_complete(&data, limits, charge, true)?;
    Ok(CompletePolicyCandidate { generation, data })
}

impl CompletePolicyCandidate {
    /// Binds an actual checked snapshot range, moving candidate and envelope owners.
    ///
    /// The range is not retained. A private outer codec must prove full content,
    /// generation/type and namespace correspondence; lengths alone provide none.
    ///
    /// # Errors
    /// Keeps new range/byte errors typed, maps current content to `InvalidAuthority`,
    /// and preserves profile/work/checked-length diagnoses without resetting work.
    pub fn bind_encoded_snapshot(
        self,
        range: Range<usize>,
        encoded: Vec<u8>,
        limits: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<CompletePolicyValue, PolicySchemaError> {
        if charge.profile() != limits {
            return Err(PolicySchemaError::ProfileMismatch);
        }
        let snapshot = encoded.get(range).ok_or(PolicySchemaError::InvalidRange)?;
        if snapshot.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicySchemaError::SnapshotBytes);
        }
        if encoded.len() > MAX_ENCODED_POLICY_BYTES {
            return Err(PolicySchemaError::EnvelopeBytes);
        }
        if self.generation == 0 || self.generation > MAX_POLICY_GENERATION {
            return Err(PolicySchemaError::InvalidAuthority);
        }
        admit_complete(&self.data, limits, charge, false).map_err(|error| match error {
            error @ (PolicySchemaError::ProfileMismatch
            | PolicySchemaError::WorkBytes { .. }
            | PolicySchemaError::LengthOverflow) => error,
            _ => PolicySchemaError::InvalidAuthority,
        })?;
        Ok(CompletePolicyValue {
            candidate: self,
            encoded,
        })
    }
}

/// Checks all current normalized content before a bounded expected-type comparison.
///
/// Success supplies no storage/namespace/encoding certificate. Immutable owners
/// reuse their constructor uniqueness proof, with every other pass charged again.
///
/// # Errors
/// Maps current content/count and expected identity refusals to `InvalidAuthority`,
/// while profile mismatch, work exhaustion and checked length overflow stay distinct.
pub fn validate_stored_complete_policy(
    value: &CompletePolicyValue,
    workflow_type: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), PolicySchemaError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch);
    }
    if value.generation() == 0
        || value.generation() > MAX_POLICY_GENERATION
        || value.encoded.len() > MAX_ENCODED_POLICY_BYTES
    {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    admit_complete(value.data(), limits, charge, false).map_err(|error| match error {
        error @ (PolicySchemaError::ProfileMismatch
        | PolicySchemaError::WorkBytes { .. }
        | PolicySchemaError::LengthOverflow) => error,
        _ => PolicySchemaError::InvalidAuthority,
    })?;
    if workflow_type.is_empty() || workflow_type.len() > MAX_POLICY_SNAPSHOT_BYTES {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    let n = u64::try_from(workflow_type.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
    charge_schema(
        charge,
        n.checked_add(128)
            .ok_or(PolicySchemaError::LengthOverflow)?,
        0,
    )?;
    if value.data().workflow_type != workflow_type {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    Ok(())
}
