//! Paid ephemeral original rendering and raw-preserving Core preparation.

use super::layout::{FlowPolicyCell as Cell, FlowPolicyLayout as Layout};
use super::{Error, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, charge_schema};
use crate::policy::complete::original_entry::indexed::{
    OriginalPolicyCurrentIndex as Index, decode_portable_retained,
};
use crate::policy::complete::{
    CompleteCorePolicyPatchInput as Input, CompletePolicyValue, StoredPolicyValue,
    verify_complete_policy_value_v2,
};
use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};

mod patch;
mod v2;
pub(super) use super::value::raw_group_parent;
use patch::policy;
pub(in crate::policy::complete) use patch::{admit_raw, encode_raw, measure_raw};
pub(super) use patch::{raw, selected};

pub(in crate::policy::complete) fn read_raw<'a>(
    raw: &'a [u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    live: usize,
) -> Result<Index<'a, 'static>, Error> {
    let index = decode_portable_retained(raw, live, schema, charge)?.ok_or(Error::Malformed)?;
    let stored = index.entry().workflow_type();
    pay(
        add(128, add(kind.len(), stored.map_or(0, <[u8]>::len))?)?,
        charge,
    )?;
    if stored != Some(kind) {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    Ok(index)
}

/// Measures a new raw branch, including its own real retained current decode.
/// The envelope bound uses maximum generation, without reading a migration value.
/// # Errors
/// Preserves actual current/profile/work errors; sizing grants no business proof.
pub fn original_core_policy_patch_bytes(
    raw: &[u8],
    input: &Input,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    additional_live: usize,
) -> Result<(usize, usize), Error> {
    if charge.profile() != schema {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    pay(128, charge)?;
    let mut index = read_raw(raw, &input.workflow_type, schema, charge, additional_live)?;
    let (snapshot, count, depth) = measure_raw(&mut index, input, None, schema, charge)?;
    Ok((
        envelope_length(snapshot, MAX_POLICY_GENERATION)?,
        bound(count, depth)?,
    ))
}

/// Prepares either stored format without making a narrowed replacement policy.
/// The standalone typed Complete entry and its logical fees remain unchanged.
/// # Errors
/// Preserves CAS, reached semantic/context, schema, wire and allocation origins.
pub fn prepare_stored_core_policy_patch(
    existing: Option<&StoredPolicyValue>,
    migration_high_water: u64,
    input: Input,
    history_max_events_limit: u32,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    additional_live: usize,
) -> Result<StoredPolicyValue, Error> {
    let Some(StoredPolicyValue::Original(raw)) = existing else {
        let current = existing.and_then(|v| match v {
            StoredPolicyValue::Complete(v) => Some(v.as_ref()),
            StoredPolicyValue::Original(_) => None,
        });
        return crate::policy::complete::prepare_complete_core_policy_patch(
            current,
            migration_high_water,
            input,
            history_max_events_limit,
            schema,
            charge,
        )
        .map(|value| StoredPolicyValue::Complete(Box::new(value)));
    };
    if charge.profile() != schema {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    pay(128, charge)?;
    let mut index = read_raw(raw, &input.workflow_type, schema, charge, additional_live)?;
    crate::policy::complete::patch::check_complete_core_patch_guard(
        index.entry().generation(),
        input.expected_generation,
    )
    .map_err(policy)?;
    admit_raw(&input, schema, charge)?;
    let measured = measure_raw(
        &mut index,
        &input,
        Some(history_max_events_limit),
        schema,
        charge,
    )?;
    if migration_high_water > MAX_POLICY_GENERATION {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    let generation = index
        .entry()
        .generation()
        .max(migration_high_water)
        .checked_add(1)
        .filter(|&g| g <= MAX_POLICY_GENERATION)
        .ok_or_else(|| policy(PolicyError::GenerationExhausted))?;
    encode_raw(
        &mut index,
        &input,
        generation,
        measured,
        schema,
        charge,
        additional_live,
    )
    .map(StoredPolicyValue::Original)
}

/// Measures an actual complete value without allocating its original projection.
/// No validation or payment credit survives this returned arithmetic.
/// # Errors
/// Preserves correspondence, profile, checked length and cumulative work errors.
pub fn original_policy_from_complete_bytes(
    value: &CompletePolicyValue,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    additional_live: usize,
) -> Result<(usize, usize), Error> {
    let (length, count, depth) = plan(value, schema, charge)?;
    let bound = bound(count, depth)?;
    additional_live.checked_add(bound).ok_or_else(overflow)?;
    Ok((length, bound))
}

/// Emits a transient, complete original envelope without changing stored backing.
/// Layout and output admissions include every caller-retained simultaneous owner.
/// # Errors
/// Preserves actual validation, size, schema and fallible allocation refusals.
pub fn encode_original_policy_from_complete(
    value: &CompletePolicyValue,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    additional_live: usize,
    maximum_encoded_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let (length, count, depth) = plan(value, schema, charge)?;
    if length > maximum_encoded_bytes || u32::try_from(length).is_err() {
        return Err(Error::ByteLimit);
    }
    let layout = Layout::new_with_base(count, depth, additional_live, charge)?;
    let mut stage = (Some(layout), 0, 0);
    let (root, snapshot) = v2::policy(value.data(), 0, &mut stage, charge)?;
    let layout = stage.0.as_ref().ok_or(Error::Malformed)?;
    peak(
        add(
            add(additional_live, bound(count, depth)?)?,
            add(512, length)?,
        )?,
        charge,
    )?;
    super::etf::emit_envelope_bounded(
        layout,
        root,
        add(1, snapshot)?,
        value.generation(),
        maximum_encoded_bytes,
        charge,
    )
}

fn plan(
    value: &CompletePolicyValue,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize, usize), Error> {
    verify_complete_policy_value_v2(value, schema, charge)?;
    let mut stage = (None, 0, 0);
    let (_, snapshot) = v2::policy(value.data(), 0, &mut stage, charge)?;
    pay(1, charge)?;
    Ok((
        envelope_length(add(1, snapshot)?, value.generation())?,
        stage.1,
        stage.2,
    ))
}

pub(super) fn envelope_length(snapshot: usize, generation: u64) -> Result<usize, Error> {
    let width = if u8::try_from(generation).is_ok() {
        2
    } else if i32::try_from(generation).is_ok() {
        5
    } else {
        add(3, generation.bit_width().div_ceil(8) as usize)?
    };
    add(snapshot, add(18, width)?)
}

pub(super) fn bound(count: usize, depth: usize) -> Result<usize, Error> {
    count
        .checked_mul(672)
        .and_then(|n| n.checked_add(depth.checked_mul(64)?))
        .and_then(|n| n.checked_add(2752))
        .ok_or_else(overflow)
}

pub(super) fn node<'a>(
    value: Cell<'a>,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let (length, probe) = match value {
        Cell::Atom(s) => (add(2, s.len())?, Some(s.len())),
        Cell::Binary(b) => (add(5, b.len())?, Some(b.len())),
        Cell::BigInteger(b) => (
            add(if b.len() <= 255 { 3 } else { 6 }, b.len())?,
            Some(b.len()),
        ),
        Cell::RawTerm(b) => (b.len(), Some(b.len())),
        Cell::Integer(n) => (
            if n <= 255 {
                2
            } else if n <= i32::MAX as u128 {
                5
            } else {
                add(3, n.bit_width().div_ceil(8) as usize)?
            },
            None,
        ),
        Cell::Map { .. } => (5, None),
        Cell::List { len, .. } => (if len == 0 { 1 } else { 6 }, None),
        Cell::Tuple { len, .. } => (if len <= 255 { 2 } else { 5 }, None),
    };
    if let Some(layout) = stage.0.as_mut() {
        return Ok((layout.push_cell(value, charge)?, length));
    }
    stage.1 = add(stage.1, 1)?;
    stage.2 = stage.2.max(depth);
    pay(128, charge)?;
    if let Some(bytes) = probe {
        pay(add(128, bytes)?, charge)?;
    }
    pay(add(128, length)?, charge)?;
    Ok((0, length))
}

pub(super) fn container(
    list: bool,
    count: usize,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize, usize), Error> {
    u32::try_from(count).map_err(|_| Error::ItemLimit)?;
    let start = stage.0.as_ref().map_or(0, |layout| layout.edges.len());
    let cell = if list {
        Cell::List { start, len: count }
    } else {
        Cell::Map { start, len: count }
    };
    let (id, length) = node(cell, depth, stage, charge)?;
    let edges = if list {
        count
    } else {
        count.checked_mul(2).ok_or_else(overflow)?
    };
    for _ in 0..edges {
        if let Some(layout) = stage.0.as_mut() {
            layout.push_edge(id, charge)?;
        } else {
            stage.1 = add(stage.1, 1)?;
            pay(128, charge)?;
        }
    }
    Ok((id, start, length))
}

pub(super) fn edge(
    slot: usize,
    child: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
) -> Result<(), Error> {
    if let Some(layout) = stage.0.as_mut() {
        if child >= layout.cells.len() {
            return Err(Error::Malformed);
        }
        *layout.edges.get_mut(slot).ok_or(Error::Malformed)? = child;
    }
    Ok(())
}

pub(super) fn fields(
    names: &[&'static str],
    values: &[(usize, usize)],
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    if names.len() != values.len() {
        return Err(Error::Malformed);
    }
    let count = values.iter().filter(|v| v.0 != usize::MAX).count();
    let (root, start, mut length) = container(false, count, depth, stage, charge)?;
    let mut ordinal = 0;
    for (&name, &(value, bytes)) in names.iter().zip(values) {
        if value == usize::MAX {
            continue;
        }
        let (key, key_bytes) = node(Cell::Atom(name), add(depth, 1)?, stage, charge)?;
        edge(add(start, ordinal * 2)?, key, stage)?;
        edge(add(start, ordinal * 2 + 1)?, value, stage)?;
        ordinal += 1;
        length = add(length, add(key_bytes, bytes)?)?;
    }
    Ok((root, length))
}

pub(super) fn strings<'a>(
    values: &'a [Vec<u8>],
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let (root, start, mut length) = container(true, values.len(), depth, stage, charge)?;
    for (ordinal, bytes) in values.iter().enumerate() {
        let (id, size) = node(Cell::Binary(bytes), add(depth, 1)?, stage, charge)?;
        edge(add(start, ordinal)?, id, stage)?;
        length = add(length, size)?;
    }
    Ok((root, length))
}

pub(super) fn absent() -> (usize, usize) {
    (usize::MAX, 0)
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b).ok_or_else(overflow)
}
pub(super) fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
pub(super) fn pay(bytes: usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(bytes).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}
pub(super) fn peak(bytes: usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    charge_schema(charge, 0, u64::try_from(bytes).map_err(|_| overflow())?).map_err(Error::Schema)
}
