//! Admission before alias comparison or construction of borrowed output spines.

use std::collections::BTreeSet;

use super::{
    ClaimCallerError, ClaimCallerLimits, ClaimDueInput, ClaimPartitionMode, ClaimPartitionsInput,
    ClaimReplyMode, ClaimStatesInput, ClaimValuesInput, ClaimValuesMode, NormalizedClaimDue,
};

/// Normalizes fixed public caller defaults and ordered aliases for comparison.
///
/// No current policy, scope, clock, selected record or apply budget is consulted.
/// Raw occurrence admission precedes every alias comparison and container.
///
/// # Errors
/// Rejects raw occurrence/count/byte excess, checked arithmetic overflow, or a
/// fallible output-spine reservation. Tree allocation retains ordinary host
/// failure semantics; this is not a universal allocation or RSS certificate.
pub fn normalize_claim_due(
    input: ClaimDueInput<'_>,
    limits: ClaimCallerLimits,
) -> Result<NormalizedClaimDue<'_>, ClaimCallerError> {
    admit_input(&input, limits)?;
    let (states_any, state_names) = normalize_states(input.states)?;
    let (partition_mode, partition_keys) = normalize_partitions(input.partitions)?;
    let (values_mode, value_names) = normalize_values(input.values)?;
    let reply_mode = input.reply_mode.unwrap_or(ClaimReplyMode::Records);
    Ok(NormalizedClaimDue {
        workflow_type: input.workflow_type,
        worker: input.worker,
        states_any,
        state_names,
        partition_mode,
        partition_keys,
        values_mode,
        value_names,
        lease_ms: input.lease_ms.unwrap_or(30_000),
        limit: input.limit.unwrap_or(1),
        priority: input.priority,
        reclaim_expired: input.reclaim_expired.unwrap_or(true),
        reclaim_ratio: input.reclaim_ratio.unwrap_or(25),
        reply_mode,
        payload_enabled: input
            .payload
            .unwrap_or(input.full.unwrap_or(reply_mode == ClaimReplyMode::Records)),
        payload_max_bytes: input.payload_max_bytes,
        governance_limit_scope: input.governance_limit_scope,
        governance_shard_id: input.governance_shard_id,
        requested_now_ms: input.requested_now_ms,
        block_ms: input.block_ms,
    })
}

fn admit_input(
    input: &ClaimDueInput<'_>,
    limits: ClaimCallerLimits,
) -> Result<(), ClaimCallerError> {
    let counts = [
        match input.states {
            ClaimStatesInput::Any => 0,
            ClaimStatesInput::One(_) => 1,
            ClaimStatesInput::Many(names) => names.len(),
        },
        match input.partitions {
            ClaimPartitionsInput::Auto
            | ClaimPartitionsInput::Any
            | ClaimPartitionsInput::Global => 0,
            ClaimPartitionsInput::One(_) => 1,
            ClaimPartitionsInput::Many(names) => names.len(),
        },
        match input.values {
            ClaimValuesInput::Omitted | ClaimValuesInput::All => 0,
            ClaimValuesInput::Names(names) => names.len(),
        },
    ];
    if counts
        .into_iter()
        .any(|count| count > limits.max_components())
    {
        return Err(ClaimCallerError::ComponentCount);
    }
    let states = match &input.states {
        ClaimStatesInput::Any => &[][..],
        ClaimStatesInput::One(name) => std::slice::from_ref(name),
        ClaimStatesInput::Many(names) => names,
    };
    let partitions = match &input.partitions {
        ClaimPartitionsInput::Auto | ClaimPartitionsInput::Any | ClaimPartitionsInput::Global => {
            &[][..]
        }
        ClaimPartitionsInput::One(name) => std::slice::from_ref(name),
        ClaimPartitionsInput::Many(names) => names,
    };
    let values = match &input.values {
        ClaimValuesInput::Omitted | ClaimValuesInput::All => &[][..],
        ClaimValuesInput::Names(names) => names,
    };
    let fixed = [
        input.workflow_type,
        input.worker,
        input.governance_limit_scope.unwrap_or_default(),
    ];
    let mut bytes: usize = 0;
    for value in fixed.iter().chain(states).chain(partitions).chain(values) {
        if value.len() > limits.max_component_bytes() {
            return Err(ClaimCallerError::ValueTooLarge {
                bytes: value.len(),
                maximum: limits.max_component_bytes(),
            });
        }
        bytes = bytes
            .checked_add(value.len())
            .ok_or(ClaimCallerError::LengthOverflow)?;
        if bytes > limits.max_raw_bytes() {
            return Err(ClaimCallerError::RawBytes {
                bytes,
                maximum: limits.max_raw_bytes(),
            });
        }
    }
    Ok(())
}

fn normalize_states(input: ClaimStatesInput<'_>) -> Result<(bool, Vec<&[u8]>), ClaimCallerError> {
    match input {
        ClaimStatesInput::Any => Ok((true, Vec::new())),
        ClaimStatesInput::One(name) => dedupe_keep_last(&[name]),
        ClaimStatesInput::Many(names) => dedupe_keep_last(names),
    }
}

fn normalize_partitions(
    input: ClaimPartitionsInput<'_>,
) -> Result<(ClaimPartitionMode, Vec<&[u8]>), ClaimCallerError> {
    match input {
        ClaimPartitionsInput::Auto => Ok((ClaimPartitionMode::Auto, Vec::new())),
        ClaimPartitionsInput::Any => Ok((ClaimPartitionMode::Any, Vec::new())),
        ClaimPartitionsInput::Global => Ok((ClaimPartitionMode::Global, Vec::new())),
        ClaimPartitionsInput::One(name) if ascii_alias(name, b"AUTO") => {
            Ok((ClaimPartitionMode::Auto, Vec::new()))
        }
        ClaimPartitionsInput::One(name) if ascii_alias(name, b"ANY") => {
            Ok((ClaimPartitionMode::Any, Vec::new()))
        }
        ClaimPartitionsInput::One(name) if ascii_alias(name, b"GLOBAL") => {
            Ok((ClaimPartitionMode::Global, Vec::new()))
        }
        ClaimPartitionsInput::One(name) => {
            Ok((ClaimPartitionMode::Only, dedupe_keep_first(&[name])?))
        }
        ClaimPartitionsInput::Many(names) => {
            Ok((ClaimPartitionMode::Only, dedupe_keep_first(names)?))
        }
    }
}

fn normalize_values(
    input: ClaimValuesInput<'_>,
) -> Result<(ClaimValuesMode, Vec<&[u8]>), ClaimCallerError> {
    match input {
        ClaimValuesInput::Omitted => Ok((ClaimValuesMode::Omitted, Vec::new())),
        ClaimValuesInput::All => Ok((ClaimValuesMode::All, Vec::new())),
        ClaimValuesInput::Names(names) => Ok((ClaimValuesMode::Only, dedupe_keep_first(names)?)),
    }
}

fn dedupe_keep_last<'a>(names: &[&'a [u8]]) -> Result<(bool, Vec<&'a [u8]>), ClaimCallerError> {
    let mut any = false;
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    for &name in names.iter().rev() {
        if ascii_alias(name, b"ANY") {
            any = true;
        } else if seen.insert(name) {
            output
                .try_reserve(1)
                .map_err(|_| ClaimCallerError::Allocation)?;
            output.push(name);
        }
    }
    output.reverse();
    Ok((any, output))
}

fn dedupe_keep_first<'a>(names: &[&'a [u8]]) -> Result<Vec<&'a [u8]>, ClaimCallerError> {
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    for &name in names {
        if seen.insert(name) {
            output
                .try_reserve(1)
                .map_err(|_| ClaimCallerError::Allocation)?;
            output.push(name);
        }
    }
    Ok(output)
}

fn ascii_alias(value: &[u8], alias: &[u8]) -> bool {
    value.eq_ignore_ascii_case(alias)
}
