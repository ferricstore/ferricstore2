//! Bounded ephemeral mode/retry owners; complete policy backing stays unchanged.

use std::collections::BTreeMap;

mod original;
pub use original::{
    original_policy_indexed_fields_equal, original_record_batch_policy_projection_bytes,
    project_original_record_batch_policy,
};

use crate::policy::definition::StatePolicy;
use crate::policy::installation::{PolicyCandidate, PolicyError};
use crate::policy::restoration::{PolicySnapshotData, restore_candidate};
use crate::policy::{InstalledRetryPolicy, RetryOverride};
use crate::retention::RetentionOverride;

use super::{
    CompletePolicyEncodingError, CompletePolicyValue, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, validate_stored_complete_policy,
};

/// Detects configured governance maps after one fresh portable raw decode.
///
/// This root/state MAP-presence restriction is not a complete resolver for
/// arbitrary raw governance. Full original bytes remain the caller's authority.
/// # Errors
/// Preserves actual profile, type binding, work, malformed and allocation errors.
pub fn original_record_batch_policy_has_governance(
    raw: &[u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, CompletePolicyEncodingError> {
    use super::flow_input::value::PolicyInputKind as Kind;
    let index = original::read(raw, kind, schema, charge)?;
    let root = index.cursor().position();
    if original::field(&index, root, b"governance", charge)?
        .is_some_and(|position| index.at(position).kind() == Kind::Map)
    {
        return Ok(true);
    }
    if let Some(states) = original::field(&index, root, b"states", charge)?
        .filter(|&position| index.at(position).kind() == Kind::Map)
    {
        for ordinal in 0..index.at(states).len() {
            super::admission::charge_schema(charge, 128, 0)
                .map_err(CompletePolicyEncodingError::Schema)?;
            let (_, config) = index
                .at(states)
                .row(ordinal)
                .ok_or(CompletePolicyEncodingError::Malformed)?;
            if config.kind() == Kind::Map
                && original::field(&index, config.position(), b"governance", charge)?
                    .is_some_and(|position| index.at(position).kind() == Kind::Map)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Measures both temporary projections, including every copied state name.
///
/// This logical tariff is independent of native capacity and complete backing.
/// # Errors
/// Rejects checked length overflow without allocating or certifying authority.
pub fn record_batch_policy_projection_bytes(
    value: &CompletePolicyValue,
) -> Result<usize, CompletePolicyEncodingError> {
    let data = value.data();
    let mut bytes = 1_024;
    add(&mut bytes, data.workflow_type.len())?;
    add(&mut bytes, data.workflow_type.len())?;
    add(&mut bytes, target_bytes(data.retry.as_ref()))?;
    for (name, state) in &data.states {
        add(&mut bytes, 256)?;
        add(&mut bytes, name.len())?;
        add(&mut bytes, name.len())?;
        add(&mut bytes, target_bytes(state.retry.as_ref()))?;
    }
    Ok(bytes)
}

/// Copies only mode/retry semantics while retaining the supplied generation.
///
/// The mode candidate omits unrelated complete fields; it is never a replacement
/// complete policy. Every state is represented in both independent projections.
/// # Errors
/// Rejects positive/owned caps before copies, schema/work refusal and allocation.
pub fn project_record_batch_policy(
    value: &CompletePolicyValue,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_owned_bytes: usize,
) -> Result<(PolicyCandidate, InstalledRetryPolicy), CompletePolicyEncodingError> {
    if maximum_owned_bytes == 0 {
        return Err(CompletePolicyEncodingError::ByteLimit);
    }
    validate_stored_complete_policy(value, &value.data().workflow_type, limits, charge)
        .map_err(CompletePolicyEncodingError::Schema)?;
    if record_batch_policy_projection_bytes(value)? > maximum_owned_bytes {
        return Err(CompletePolicyEncodingError::ByteLimit);
    }
    let data = value.data();
    let mut modes = BTreeMap::new();
    let mut retries = BTreeMap::new();
    for (name, state) in &data.states {
        modes.insert(
            copy(name)?,
            StatePolicy {
                mode: state.mode,
                retry: RetryOverride::default(),
                retention: RetentionOverride::default(),
            },
        );
        retries.insert(copy(name)?, retry_copy(state.retry.as_ref())?);
    }
    let mode = restore_candidate(
        value.generation(),
        PolicySnapshotData {
            workflow_type: copy(&data.workflow_type)?,
            version: None,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states: modes,
        },
        limits.policy_limits(),
    )
    .map_err(|error| CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(error)))?;
    let retry = InstalledRetryPolicy::new(
        copy(&data.workflow_type)?,
        retry_copy(data.retry.as_ref())?,
        retries,
    )
    .map_err(|error| {
        CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(PolicyError::Domain(error)))
    })?;
    Ok((mode, retry))
}

fn target_bytes(retry: Option<&RetryOverride>) -> usize {
    retry
        .and_then(|value| value.exhausted_to.as_ref())
        .map_or(0, Vec::len)
}

fn retry_copy(retry: Option<&RetryOverride>) -> Result<RetryOverride, CompletePolicyEncodingError> {
    let Some(retry) = retry else {
        return Ok(RetryOverride::default());
    };
    Ok(RetryOverride {
        max_retries: retry.max_retries,
        backoff: retry.backoff,
        exhausted_to: retry.exhausted_to.as_deref().map(copy).transpose()?,
    })
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, CompletePolicyEncodingError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
    output.extend_from_slice(bytes);
    Ok(output)
}

fn add(total: &mut usize, bytes: usize) -> Result<(), CompletePolicyEncodingError> {
    *total = total
        .checked_add(bytes)
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    Ok(())
}
