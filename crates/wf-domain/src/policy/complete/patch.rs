//! Normalized partial core preparation without installation authority.

use std::collections::BTreeMap;

use crate::policy::definition::PolicyPatch;
use crate::policy::installation::{MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES, PolicyError};
use crate::value::{DomainError, InputField};

use super::admission::charge_schema;
use super::encoding::decode_complete_policy_candidate_v2;
use super::validation::admit_complete;
use super::{
    CompletePolicyCandidate, CompletePolicyData, CompletePolicyEncodingError, CompletePolicyValue,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, prepare_complete_policy_value_v2,
    validate_stored_complete_policy, verify_complete_policy_value_v2,
};

mod merge;

/// Owned, unvalidated partial core policy command.
pub struct CompleteCorePolicyPatchInput {
    /// Opaque workflow type supplied by the caller.
    pub workflow_type: Vec<u8>,
    /// Optional compare-and-set generation.
    pub expected_generation: Option<u64>,
    /// Supported normalized partial core fields.
    pub patch: PolicyPatch,
}

/// Prepares a presence-preserving core Patch against one observed complete value.
///
/// Current semantic owners are copied through paid V2 restoration; incoming owners
/// move into the result. The same account retains every earlier admission and peak.
/// This normalized value provides no partial-command, namespace or install authority.
///
/// # Errors
/// Preserves profile, CAS, current correspondence and new typed merge refusals.
/// Contextual validation precedes migration bounds and generation allocation.
pub fn prepare_complete_core_policy_patch(
    existing: Option<&CompletePolicyValue>,
    migration_high_water: u64,
    input: CompleteCorePolicyPatchInput,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    if charge.profile() != limits {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    charge_schema(charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)?;
    let stored = existing.map_or(0, CompletePolicyValue::generation);
    if let Err(error) = check_complete_core_patch_guard(stored, input.expected_generation) {
        let error = match error {
            PolicyError::InvalidAuthority => PolicySchemaError::InvalidAuthority,
            error => PolicySchemaError::Policy(error),
        };
        return Err(CompletePolicyEncodingError::Schema(error));
    }
    if let Some(existing) = existing {
        validate_complete_core_patch_current(existing, &input.workflow_type, limits, charge)?;
    }
    let data = restore_core_patch_current(existing, input.workflow_type, limits, charge)?;
    let data = merge_core_patch_content(
        data,
        input.patch,
        existing.is_some(),
        history_max_events_limit,
        limits,
        charge,
    )?;
    encode_core_patch_content(stored, migration_high_water, data, limits, charge)
}

pub(super) fn check_complete_core_patch_guard(
    stored: u64,
    expected: Option<u64>,
) -> Result<(), PolicyError> {
    if stored > MAX_POLICY_GENERATION {
        return Err(PolicyError::InvalidAuthority);
    }
    if let Some(expected) = expected {
        if expected > MAX_POLICY_GENERATION {
            return Err(PolicyError::InvalidGeneration);
        }
        if expected != stored {
            return Err(PolicyError::StaleGeneration);
        }
    }
    Ok(())
}

pub(super) fn validate_complete_core_patch_current(
    value: &CompletePolicyValue,
    workflow_type: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyEncodingError> {
    validate_stored_complete_policy(value, workflow_type, limits, charge)
        .map_err(CompletePolicyEncodingError::Schema)?;
    verify_complete_policy_value_v2(value, limits, charge)
}

pub(super) fn restore_core_patch_current(
    existing: Option<&CompletePolicyValue>,
    workflow_type: Vec<u8>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyData, CompletePolicyEncodingError> {
    if let Some(existing) = existing {
        let candidate = decode_complete_policy_candidate_v2(existing.encoded(), limits, charge)?;
        let mut data = candidate.data;
        data.workflow_type = workflow_type;
        Ok(data)
    } else {
        charge_schema(charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)?;
        if workflow_type.is_empty() {
            return Err(CompletePolicyEncodingError::Schema(
                PolicySchemaError::Policy(PolicyError::Domain(DomainError::EmptyValue(
                    InputField::Type,
                ))),
            ));
        }
        if workflow_type.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(CompletePolicyEncodingError::Schema(
                PolicySchemaError::SnapshotBytes,
            ));
        }
        charge_schema(charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)?;
        Ok(CompletePolicyData {
            workflow_type,
            version: None,
            max_active_ms: None,
            retry: None,
            retention: None,
            states: BTreeMap::new(),
            indexed_attributes: None,
            indexed_state_meta: None,
            governance: None,
        })
    }
}

pub(super) fn merge_core_patch_content(
    mut data: CompletePolicyData,
    patch: PolicyPatch,
    had_existing: bool,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyData, CompletePolicyEncodingError> {
    merge::merge_core_policy(
        &mut data,
        patch,
        had_existing,
        history_max_events_limit,
        limits,
        charge,
    )
    .map_err(CompletePolicyEncodingError::Schema)?;
    admit_complete(&data, limits, charge, true).map_err(CompletePolicyEncodingError::Schema)?;
    Ok(data)
}

pub(super) fn encode_core_patch_content(
    stored: u64,
    migration_high_water: u64,
    data: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, CompletePolicyEncodingError> {
    if migration_high_water > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority,
        ));
    }
    let generation = stored.max(migration_high_water).checked_add(1).ok_or(
        CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
            PolicyError::GenerationExhausted,
        )),
    )?;
    if generation > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::GenerationExhausted),
        ));
    }
    prepare_complete_policy_value_v2(CompletePolicyCandidate { generation, data }, limits, charge)
}
