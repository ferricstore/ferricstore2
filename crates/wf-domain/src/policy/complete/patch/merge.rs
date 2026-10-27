//! Presence-preserving core merges through one continuous schema account.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use crate::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StatePolicyPatch,
};
use crate::policy::installation::{MAX_POLICY_SNAPSHOT_BYTES, PolicyError};
use crate::policy::{Backoff, RetryOverride};
use crate::retention::{MAX_RETENTION_MILLIS, RetentionOverride};
use crate::value::{DomainError, RetryPolicyField};

use super::super::admission::charge_schema;
use super::super::{
    CompletePolicyData, CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits,
};

pub(super) fn merge_core_policy(
    data: &mut CompletePolicyData,
    patch: PolicyPatch,
    had_existing: bool,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), PolicySchemaError> {
    charge_schema(charge, 128, 0)?;
    charge_schema(charge, 128, 0)?;
    match patch.version {
        FieldPatch::Unchanged => {}
        FieldPatch::Clear => data.version = None,
        FieldPatch::Set(version) => {
            if let PolicyVersion::Bytes(bytes) = &version {
                if bytes.len() > MAX_POLICY_SNAPSHOT_BYTES {
                    return Err(PolicySchemaError::SnapshotBytes);
                }
                let n = u64::try_from(bytes.len())
                    .ok()
                    .ok_or(PolicySchemaError::LengthOverflow)?;
                let work = n
                    .checked_add(128)
                    .ok_or(PolicySchemaError::LengthOverflow)?;
                charge_schema(charge, work, 0)?;
                if bytes.is_empty() {
                    return Err(PolicySchemaError::Policy(PolicyError::InvalidVersion));
                }
            }
            data.version = Some(version);
        }
    }
    charge_schema(charge, 128, 0)?;
    match patch.max_active_ms {
        FieldPatch::Unchanged => {}
        FieldPatch::Clear => data.max_active_ms = None,
        FieldPatch::Set(active) => {
            if active == 0 || active > MAX_RETENTION_MILLIS {
                return Err(PolicySchemaError::Policy(PolicyError::InvalidMaxActive));
            }
            data.max_active_ms = Some(active);
        }
    }
    charge_schema(charge, 128, 0)?;
    merge_core_retry(&mut data.retry, patch.retry, charge)?;
    charge_schema(charge, 128, 0)?;
    merge_core_retention(
        &mut data.retention,
        &patch.retention,
        history_max_events_limit,
        charge,
    )?;
    charge_schema(charge, 128, 0)?;
    merge_core_states(
        &mut data.states,
        patch.states,
        had_existing,
        history_max_events_limit,
        limits,
        charge,
    )?;
    Ok(())
}

fn merge_core_states(
    current: &mut BTreeMap<Vec<u8>, CompleteStatePolicyData>,
    patch: BTreeMap<Vec<u8>, StateChange>,
    had_existing: bool,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), PolicySchemaError> {
    final_core_state_count(current, &patch, had_existing, limits, charge)?;
    for (name, change) in &patch {
        charge_schema(charge, 128, 0)?;
        if matches!(change, StateChange::Remove) {
            if name.len() > MAX_POLICY_SNAPSHOT_BYTES {
                return Err(PolicySchemaError::SnapshotBytes);
            }
            let n = u64::try_from(name.len())
                .ok()
                .ok_or(PolicySchemaError::LengthOverflow)?;
            let work = n
                .checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?;
            charge_schema(charge, work, 0)?;
            charge_schema(charge, work, 0)?;
            current.remove(name);
        }
    }
    for (name, change) in patch {
        charge_schema(charge, 128, 0)?;
        let StateChange::Patch(patch) = change else {
            continue;
        };
        if name.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicySchemaError::SnapshotBytes);
        }
        let n = u64::try_from(name.len())
            .ok()
            .ok_or(PolicySchemaError::LengthOverflow)?;
        let work = n
            .checked_add(128)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        charge_schema(charge, work, 0)?;
        if name.is_empty() || name == b"running" {
            let field = if name.is_empty() {
                RetryPolicyField::State
            } else {
                RetryPolicyField::RunningState
            };
            return Err(PolicySchemaError::Policy(PolicyError::Domain(
                DomainError::InvalidRetryPolicy(field),
            )));
        }
        charge_schema(charge, work, 0)?;
        let state = match current.entry(name) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                charge_schema(charge, 128, 0)?;
                entry.insert(CompleteStatePolicyData {
                    mode: None,
                    retry: None,
                    retention: None,
                    governance: None,
                })
            }
        };
        merge_core_state(state, patch, history_max_events_limit, charge)?;
    }
    charge_schema(charge, 128, 0)?;
    for (name, state) in current {
        charge_schema(charge, 128, 0)?;
        if name.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicySchemaError::SnapshotBytes);
        }
        let n = u64::try_from(name.len())
            .ok()
            .ok_or(PolicySchemaError::LengthOverflow)?;
        let work = n
            .checked_add(128)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        charge_schema(charge, work, 0)?;
        merge_core_retention(
            &mut state.retention,
            &FieldPatch::Unchanged,
            history_max_events_limit,
            charge,
        )?;
    }
    Ok(())
}

fn final_core_state_count(
    current: &BTreeMap<Vec<u8>, CompleteStatePolicyData>,
    patch: &BTreeMap<Vec<u8>, StateChange>,
    had_existing: bool,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, PolicySchemaError> {
    let mut removed = 0_usize;
    let mut added = 0_usize;
    for (name, change) in patch {
        charge_schema(charge, 128, 0)?;
        if name.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicySchemaError::SnapshotBytes);
        }
        let n = u64::try_from(name.len())
            .ok()
            .ok_or(PolicySchemaError::LengthOverflow)?;
        let work = n
            .checked_add(128)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        charge_schema(charge, work, 0)?;
        if matches!(change, StateChange::Remove) && !had_existing {
            return Err(PolicySchemaError::Policy(PolicyError::InvalidStatePolicy));
        }
        charge_schema(charge, work, 0)?;
        match (change, current.contains_key(name)) {
            (StateChange::Remove, true) => {
                removed = removed
                    .checked_add(1)
                    .ok_or(PolicySchemaError::LengthOverflow)?;
            }
            (StateChange::Patch(_), false) => {
                added = added
                    .checked_add(1)
                    .ok_or(PolicySchemaError::LengthOverflow)?;
            }
            _ => {}
        }
    }
    let count = current
        .len()
        .checked_sub(removed)
        .ok_or(PolicySchemaError::LengthOverflow)?
        .checked_add(added)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    if count > limits.policy_limits().max_states() {
        return Err(PolicySchemaError::Policy(PolicyError::StateBudget));
    }
    Ok(count)
}

fn merge_core_state(
    current: &mut CompleteStatePolicyData,
    patch: StatePolicyPatch,
    history_max_events_limit: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<(), PolicySchemaError> {
    charge_schema(charge, 128, 0)?;
    match patch.mode {
        FieldPatch::Unchanged => {}
        FieldPatch::Set(mode) => current.mode = Some(mode),
        FieldPatch::Clear => return Err(PolicySchemaError::Policy(PolicyError::InvalidStateMode)),
    }
    charge_schema(charge, 128, 0)?;
    merge_core_retry(&mut current.retry, patch.retry, charge)?;
    charge_schema(charge, 128, 0)?;
    merge_core_retention(
        &mut current.retention,
        &patch.retention,
        history_max_events_limit,
        charge,
    )
}

fn merge_core_retry(
    current: &mut Option<RetryOverride>,
    patch: FieldPatch<RetryPatch>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), PolicySchemaError> {
    match patch {
        FieldPatch::Unchanged => {}
        FieldPatch::Clear => *current = None,
        FieldPatch::Set(patch) => {
            charge_schema(charge, 128, 0)?;
            let current = current.get_or_insert(RetryOverride::default());
            charge_schema(charge, 128, 0)?;
            if let Some(maximum) = patch.max_retries {
                current.max_retries = Some(maximum);
            }
            charge_schema(charge, 128, 0)?;
            if let Some(backoff) = patch.backoff {
                current.backoff = Some(merge_core_backoff(current.backoff, backoff, charge)?);
            }
            charge_schema(charge, 128, 0)?;
            if let Some(target) = patch.exhausted_to {
                if target.len() > MAX_POLICY_SNAPSHOT_BYTES {
                    return Err(PolicySchemaError::SnapshotBytes);
                }
                let n = u64::try_from(target.len())
                    .ok()
                    .ok_or(PolicySchemaError::LengthOverflow)?;
                let work = n
                    .checked_add(128)
                    .ok_or(PolicySchemaError::LengthOverflow)?;
                charge_schema(charge, work, 0)?;
                current.exhausted_to = Some(target);
            }
        }
    }
    if let Some(current) = current {
        for _ in 0..3 {
            charge_schema(charge, 128, 0)?;
        }
        if current.backoff.is_some() {
            for _ in 0..5 {
                charge_schema(charge, 128, 0)?;
            }
        }
        charge_schema(charge, 128, 0)?;
        if let Some(target) = &current.exhausted_to {
            if target.len() > MAX_POLICY_SNAPSHOT_BYTES {
                return Err(PolicySchemaError::SnapshotBytes);
            }
            let n = u64::try_from(target.len())
                .ok()
                .ok_or(PolicySchemaError::LengthOverflow)?;
            let work = n
                .checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?;
            charge_schema(charge, work, 0)?;
        }
        current
            .validate()
            .map_err(PolicyError::Domain)
            .map_err(PolicySchemaError::Policy)?;
    }
    Ok(())
}

fn merge_core_backoff(
    current: Option<Backoff>,
    patch: BackoffPatch,
    charge: &mut PolicySchemaCharge,
) -> Result<Backoff, PolicySchemaError> {
    charge_schema(charge, 128, 0)?;
    let mut current = current.unwrap_or_default();
    charge_schema(charge, 128, 0)?;
    if let Some(kind) = patch.kind {
        current.kind = kind;
    }
    charge_schema(charge, 128, 0)?;
    if let Some(base) = patch.base_ms {
        current.base_ms = base;
    }
    charge_schema(charge, 128, 0)?;
    if let Some(maximum) = patch.max_ms {
        current.max_ms = maximum;
    }
    charge_schema(charge, 128, 0)?;
    if let Some(jitter) = patch.jitter_pct {
        current.jitter_pct = jitter;
    }
    Ok(current)
}

fn merge_core_retention(
    current: &mut Option<RetentionOverride>,
    patch: &FieldPatch<RetentionOverride>,
    history_max_events_limit: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<(), PolicySchemaError> {
    match patch {
        FieldPatch::Unchanged => {}
        FieldPatch::Clear => *current = None,
        FieldPatch::Set(patch) => {
            charge_schema(charge, 128, 0)?;
            let current = current.get_or_insert(RetentionOverride::default());
            charge_schema(charge, 128, 0)?;
            if let Some(ttl) = patch.ttl_ms {
                current.ttl_ms = Some(ttl);
            }
            charge_schema(charge, 128, 0)?;
            if let Some(history) = patch.history_max_events {
                current.history_max_events = Some(history);
            }
        }
    }
    if let Some(current) = current {
        for _ in 0..3 {
            charge_schema(charge, 128, 0)?;
        }
        current
            .validate(history_max_events_limit)
            .map_err(PolicyError::Retention)
            .map_err(PolicySchemaError::Policy)?;
    }
    Ok(())
}
