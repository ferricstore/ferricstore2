//! Creation capture over a retained complete or original stored-policy donor.

use super::flow_input::value::{PolicyInputCursor as Cursor, PolicyInputKind as Kind, unsigned};
use super::original_entry::{
    TermPosition,
    indexed::{OriginalPolicyCurrentIndex as Index, decode_portable_retained},
};
use super::{
    CompletePolicyEncodingError, CompletePolicyEncodingError as Error, PolicySchemaCharge,
    PolicySchemaError as SchemaError, PolicySchemaLimits, StoredPolicyValue, charge_schema,
    validate_stored_complete_policy,
};
use crate::retention::{MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS, RetentionOverride};
use crate::workflow::active::{
    ActiveValueError, CapturedLifecycle, LifecycleDefaults, LifecycleOverrides,
    capture_lifecycle_layers,
};

/// Ordinary creation inputs; these values certify no policy or storage authority.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct StoredLifecycleCaptureInput<'input> {
    /// Workflow type bound to the actual retained policy donor.
    pub workflow_type: &'input [u8],
    /// Initial state whose lifecycle layer precedes explicit command overrides.
    pub initial_state: &'input [u8],
    /// Presence-preserving command lifecycle fields.
    pub overrides: LifecycleOverrides,
    /// Trusted configured defaults and clamping bounds.
    pub defaults: LifecycleDefaults,
}

/// Distinct stored-policy operational and creation-value refusal origins.
#[derive(Debug)]
pub enum StoredLifecycleCaptureError {
    /// Stored backing, profile, schema work or temporary-owner refusal.
    Policy(CompletePolicyEncodingError),
    /// Invalid reached scalar defaults or command overrides.
    Capture(ActiveValueError),
}

/// Resolves creation scalars from a retained full policy under one work account.
///
/// Complete fields remain borrowed. Original uses one bounded temporary index;
/// only scalar layers cross its lifetime. The output certifies no transaction.
///
/// # Errors
/// Preserves operational policy/profile/work refusals separately from invalid
/// reached creation defaults or command overrides. Admitted work is never reset.
pub fn capture_stored_creation_lifecycle(
    policy: Option<&StoredPolicyValue>,
    input: StoredLifecycleCaptureInput<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_temporary_bytes: usize,
) -> Result<CapturedLifecycle, StoredLifecycleCaptureError> {
    if charge.profile() != schema {
        return Err(StoredLifecycleCaptureError::Policy(Error::Schema(
            SchemaError::ProfileMismatch,
        )));
    }
    charge_schema(charge, 16_384, 0)
        .map_err(|e| StoredLifecycleCaptureError::Policy(Error::Schema(e)))?;
    let (policy_type, active, type_retention, state_retention) = match policy {
        None => (None, None, RetentionOverride::default(), None),
        Some(StoredPolicyValue::Complete(value)) => {
            validate_stored_complete_policy(value, input.workflow_type, schema, charge)
                .map_err(|e| StoredLifecycleCaptureError::Policy(Error::Schema(e)))?;
            let data = value.data();
            let mut local = None;
            let mut rows = data.states.iter();
            for _ in 0..data.states.len() {
                charge_schema(charge, 384, 0)
                    .map_err(|e| StoredLifecycleCaptureError::Policy(Error::Schema(e)))?;
                let (name, state) =
                    rows.next()
                        .ok_or(StoredLifecycleCaptureError::Policy(Error::Schema(
                            SchemaError::InvalidAuthority,
                        )))?;
                let work = name
                    .len()
                    .checked_add(input.initial_state.len())
                    .and_then(|n| n.checked_add(128))
                    .and_then(|n| u64::try_from(n).ok())
                    .ok_or(StoredLifecycleCaptureError::Policy(Error::Schema(
                        SchemaError::LengthOverflow,
                    )))?;
                charge_schema(charge, work, 0)
                    .map_err(|e| StoredLifecycleCaptureError::Policy(Error::Schema(e)))?;
                if name.as_slice() == input.initial_state {
                    local = state.retention;
                    break;
                }
            }
            (
                Some(data.workflow_type.as_slice()),
                data.max_active_ms,
                data.retention.unwrap_or_default(),
                local,
            )
        }
        Some(StoredPolicyValue::Original(raw)) => {
            let (active, type_retention, state_retention) =
                read_original(raw, input, schema, charge, maximum_temporary_bytes)
                    .map_err(StoredLifecycleCaptureError::Policy)?;
            (
                Some(input.workflow_type),
                active,
                type_retention,
                state_retention,
            )
        }
    };
    if let Some(kind) = policy_type {
        let work = kind
            .len()
            .checked_add(input.workflow_type.len())
            .and_then(|n| n.checked_add(128))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(StoredLifecycleCaptureError::Policy(Error::Schema(
                SchemaError::LengthOverflow,
            )))?;
        charge_schema(charge, work, 0)
            .map_err(|e| StoredLifecycleCaptureError::Policy(Error::Schema(e)))?;
    }
    capture_lifecycle_layers(
        policy_type,
        input.workflow_type,
        active,
        type_retention,
        state_retention,
        input.overrides,
        input.defaults,
    )
    .map_err(StoredLifecycleCaptureError::Capture)
}

fn read_original(
    raw: &[u8],
    input: StoredLifecycleCaptureInput<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    max_temp: usize,
) -> Result<(Option<u64>, RetentionOverride, Option<RetentionOverride>), Error> {
    charge_schema(charge, 4_096, 0).map_err(Error::Schema)?;
    let bound = raw
        .len()
        .checked_mul(256)
        .and_then(|n| n.checked_add(65_536))
        .ok_or(Error::Schema(SchemaError::LengthOverflow))?;
    if bound > max_temp {
        return Err(Error::ByteLimit);
    }
    let mut index = decode_portable_retained(raw, 0, schema, charge)?.ok_or(Error::Malformed)?;
    let actual = index.entry().workflow_type();
    let work = input
        .workflow_type
        .len()
        .checked_add(actual.map_or(0, <[u8]>::len))
        .and_then(|n| n.checked_add(128))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(Error::Schema(SchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)?;
    if actual != Some(input.workflow_type) {
        return Err(Error::Schema(SchemaError::InvalidAuthority));
    }
    let live = raw
        .len()
        .checked_mul(192)
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(SchemaError::LengthOverflow))?;
    index.build_catalog(live, charge)?;
    let root = index.cursor().position();
    let active = match field(&index, root, b"max_active_ms", true, charge)? {
        Some(p) => number(&index, p, charge)?,
        None => None,
    };
    if active.is_some_and(|n| n == 0 || n > MAX_RETENTION_MILLIS) {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let type_retention = retention(
        &index,
        field(&index, root, b"retention", true, charge)?,
        charge,
    )?;
    let selected = match field(&index, root, b"states", true, charge)? {
        Some(states) => field(&index, states, input.initial_state, false, charge)?,
        None => None,
    };
    let state_retention = match selected {
        Some(config) => Some(retention(
            &index,
            field(&index, config, b"retention", true, charge)?,
            charge,
        )?),
        None => None,
    };
    Ok((active, type_retention, state_retention))
}

fn field(
    index: &Index<'_, '_>,
    map: TermPosition,
    name: &[u8],
    atom_key: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<TermPosition>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let map = index.at(map);
    if map.kind() != Kind::Map {
        return Ok(None);
    }
    for ordinal in 0..map.len() {
        charge_schema(charge, 384, 0).map_err(Error::Schema)?;
        let (key, value) = map.row(ordinal).ok_or(Error::Malformed)?;
        let bytes = if atom_key {
            key.atom_bytes()
        } else {
            key.binary_bytes()
        };
        if let Some(bytes) = bytes {
            let work = bytes
                .len()
                .checked_add(name.len())
                .and_then(|n| n.checked_add(128))
                .and_then(|n| u64::try_from(n).ok())
                .ok_or(Error::Schema(SchemaError::LengthOverflow))?;
            charge_schema(charge, work, 0).map_err(Error::Schema)?;
            if bytes == name {
                return Ok(Some(value.position()));
            }
        }
    }
    Ok(None)
}

fn retention(
    index: &Index<'_, '_>,
    position: Option<TermPosition>,
    charge: &mut PolicySchemaCharge,
) -> Result<RetentionOverride, Error> {
    charge_schema(charge, 4_096, 0).map_err(Error::Schema)?;
    let Some(map) = position else {
        return Ok(RetentionOverride::default());
    };
    if index.at(map).kind() != Kind::Map {
        return Ok(RetentionOverride::default());
    }
    let ttl_ms = match field(index, map, b"ttl_ms", true, charge)? {
        Some(p) => Some(number(index, p, charge)?.ok_or(Error::UnsupportedPolicyProfile)?),
        None => None,
    };
    if field(index, map, b"history_hot_max_events", true, charge)?.is_some() {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let history_max_events = match field(index, map, b"history_max_events", true, charge)? {
        Some(p) => {
            let n = number(index, p, charge)?.ok_or(Error::UnsupportedPolicyProfile)?;
            Some(u32::try_from(n).map_err(|_| Error::UnsupportedPolicyProfile)?)
        }
        None => None,
    };
    let result = RetentionOverride {
        ttl_ms,
        history_max_events,
    };
    result
        .validate(MAX_HISTORY_EVENTS)
        .map_err(|_| Error::UnsupportedPolicyProfile)?;
    Ok(result)
}

fn number(
    index: &Index<'_, '_>,
    position: TermPosition,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u64>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let value = index.at(position);
    if value.kind() != Kind::Integer {
        return Ok(None);
    }
    let TermPosition::Encoded(id) = value.position() else {
        return Err(Error::UnsupportedPolicyProfile);
    };
    let length = u64::try_from(index.raw_span(id)?.len())
        .map_err(|_| Error::Schema(SchemaError::LengthOverflow))?;
    charge_schema(charge, length, 0).map_err(Error::Schema)?;
    unsigned(Cursor::Current(value), u64::MAX, charge)?
        .map(Some)
        .ok_or(Error::UnsupportedPolicyProfile)
}
