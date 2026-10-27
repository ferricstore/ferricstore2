//! Pre-copy policy field/count/raw-byte admission; encoded bytes checked separately.

use crate::retention::{MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS};
use crate::value::{DomainError, InputField, RetryPolicyField};

use std::collections::BTreeMap;

use super::Backoff;
use super::definition::{
    FieldPatch, PolicySnapshot, PolicyVersion, RetryPatch, StateChange, StatePolicyPatch,
};
use super::installation::{
    InstallCommand, InstallMode, MAX_POLICY_SNAPSHOT_BYTES, PolicyError, PolicyLimits,
};

pub(super) fn validate_patch(
    command: &InstallCommand,
    limits: PolicyLimits,
) -> Result<(), PolicyError> {
    if command.workflow_type.is_empty() {
        return Err(PolicyError::Domain(DomainError::EmptyValue(
            InputField::Type,
        )));
    }
    if command.patch.states.len() > limits.max_states() {
        return Err(PolicyError::StateBudget);
    }
    if let FieldPatch::Set(version) = &command.patch.version {
        validate_version(version)?;
    }
    if let FieldPatch::Set(active) = command.patch.max_active_ms {
        validate_active(active)?;
    }
    let mut bytes = 0;
    add(&mut bytes, command.workflow_type.len())?;
    for (state, change) in &command.patch.states {
        validate_state(state)?;
        add(&mut bytes, state.len())?;
        if command.mode == InstallMode::Replace && matches!(change, StateChange::Remove) {
            return Err(PolicyError::InvalidStatePolicy);
        }
        let StateChange::Patch(policy) = change else {
            continue;
        };
        if matches!(policy.mode, FieldPatch::Clear) {
            return Err(PolicyError::InvalidStateMode);
        }
        if let FieldPatch::Set(retention) = policy.retention {
            retention
                .validate(MAX_HISTORY_EVENTS)
                .map_err(PolicyError::Retention)?;
        }
        if let FieldPatch::Set(retry) = &policy.retry {
            validate_retry_patch(retry)?;
            if let Some(target) = &retry.exhausted_to {
                add(&mut bytes, target.len())?;
            }
        }
    }
    if let FieldPatch::Set(retry) = &command.patch.retry {
        validate_retry_patch(retry)?;
        if let Some(target) = &retry.exhausted_to {
            add(&mut bytes, target.len())?;
        }
    }
    if let FieldPatch::Set(retention) = command.patch.retention {
        retention
            .validate(MAX_HISTORY_EVENTS)
            .map_err(PolicyError::Retention)?;
    }
    if let FieldPatch::Set(PolicyVersion::Bytes(version)) = &command.patch.version {
        add(&mut bytes, version.len())?;
    }
    Ok(())
}

pub(super) fn validate_definition(
    workflow_type: &[u8],
    states: &BTreeMap<Vec<u8>, StatePolicyPatch>,
    limits: PolicyLimits,
) -> Result<(), PolicyError> {
    if workflow_type.is_empty() {
        return Err(PolicyError::Domain(DomainError::EmptyValue(
            InputField::Type,
        )));
    }
    if states.len() > limits.max_states() {
        return Err(PolicyError::StateBudget);
    }
    let mut bytes = 0;
    add(&mut bytes, workflow_type.len())?;
    for (state, policy) in states {
        validate_state(state)?;
        add(&mut bytes, state.len())?;
        if matches!(policy.mode, FieldPatch::Clear) {
            return Err(PolicyError::InvalidStateMode);
        }
        if let FieldPatch::Set(retry) = &policy.retry {
            validate_retry_patch(retry)?;
            if let Some(target) = &retry.exhausted_to {
                add(&mut bytes, target.len())?;
            }
        }
        if let FieldPatch::Set(retention) = policy.retention {
            retention
                .validate(MAX_HISTORY_EVENTS)
                .map_err(PolicyError::Retention)?;
        }
    }
    Ok(())
}

fn validate_retry_patch(retry: &RetryPatch) -> Result<(), PolicyError> {
    if retry.max_retries.is_some_and(|limit| limit > 1_000) {
        return Err(PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::MaxRetries,
        )));
    }
    if let Some(target) = &retry.exhausted_to {
        let field = if target.is_empty() {
            Some(RetryPolicyField::ExhaustedTo)
        } else if target == b"running" {
            Some(RetryPolicyField::RunningExhaustion)
        } else {
            None
        };
        if let Some(field) = field {
            return Err(PolicyError::Domain(DomainError::InvalidRetryPolicy(field)));
        }
    }
    if let Some(patch) = retry.backoff {
        let defaults = Backoff::default();
        Backoff {
            kind: patch.kind.unwrap_or(defaults.kind),
            base_ms: patch.base_ms.unwrap_or(defaults.base_ms),
            max_ms: patch.max_ms.unwrap_or(defaults.max_ms),
            jitter_pct: patch.jitter_pct.unwrap_or(defaults.jitter_pct),
        }
        .validate()
        .map_err(PolicyError::Domain)?;
    }
    Ok(())
}

pub(super) fn validate_snapshot(
    policy: &PolicySnapshot,
    limits: PolicyLimits,
) -> Result<(), PolicyError> {
    if policy.workflow_type.is_empty() {
        return Err(PolicyError::Domain(DomainError::EmptyValue(
            InputField::Type,
        )));
    }
    if policy.states.len() > limits.max_states() {
        return Err(PolicyError::StateBudget);
    }
    if let Some(version) = &policy.version {
        validate_version(version)?;
    }
    if let Some(active) = policy.max_active_ms {
        validate_active(active)?;
    }
    policy.retry.validate().map_err(PolicyError::Domain)?;
    policy
        .retention
        .validate(MAX_HISTORY_EVENTS)
        .map_err(PolicyError::Retention)?;
    let mut bytes = 0;
    add(&mut bytes, policy.workflow_type.len())?;
    if let Some(PolicyVersion::Bytes(version)) = &policy.version {
        add(&mut bytes, version.len())?;
    }
    if let Some(target) = &policy.retry.exhausted_to {
        add(&mut bytes, target.len())?;
    }
    for (state, policy) in &policy.states {
        validate_state(state)?;
        policy.retry.validate().map_err(PolicyError::Domain)?;
        policy
            .retention
            .validate(MAX_HISTORY_EVENTS)
            .map_err(PolicyError::Retention)?;
        add(&mut bytes, state.len())?;
        if let Some(target) = &policy.retry.exhausted_to {
            add(&mut bytes, target.len())?;
        }
    }
    if bytes > MAX_POLICY_SNAPSHOT_BYTES {
        return Err(PolicyError::SnapshotTooLarge);
    }
    Ok(())
}

fn validate_version(version: &PolicyVersion) -> Result<(), PolicyError> {
    if matches!(version, PolicyVersion::Bytes(bytes) if bytes.is_empty()) {
        Err(PolicyError::InvalidVersion)
    } else {
        Ok(())
    }
}
fn validate_active(active: u64) -> Result<(), PolicyError> {
    if active == 0 || active > MAX_RETENTION_MILLIS {
        Err(PolicyError::InvalidMaxActive)
    } else {
        Ok(())
    }
}
fn validate_state(state: &[u8]) -> Result<(), PolicyError> {
    let field = if state.is_empty() {
        Some(RetryPolicyField::State)
    } else if state == b"running" {
        Some(RetryPolicyField::RunningState)
    } else {
        None
    };
    match field {
        Some(field) => Err(PolicyError::Domain(DomainError::InvalidRetryPolicy(field))),
        None => Ok(()),
    }
}
fn add(bytes: &mut usize, value: usize) -> Result<(), PolicyError> {
    *bytes = bytes
        .checked_add(value)
        .ok_or(PolicyError::SnapshotTooLarge)?;
    if *bytes > MAX_POLICY_SNAPSHOT_BYTES {
        Err(PolicyError::SnapshotTooLarge)
    } else {
        Ok(())
    }
}
