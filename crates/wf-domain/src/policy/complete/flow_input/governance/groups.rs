//! Source effects, audit and approvals without filling resolved defaults.

use super::{Cell, Cursor, Error, Field, Gov, Layout, Map, PolicySchemaCharge};
use super::{classify_map, enumeration, positive, select, visit};

pub(super) fn effects<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    let Some(input) = classify_map(value, charge)? else {
        return Ok(Err(Gov::EffectsShape));
    };
    let mut ids = [usize::MAX; 3];
    ids[0] = match super::lists::optional(input, "allowed", Field::Allowed, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[1] = match super::lists::optional(input, "denied", Field::Denied, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[2] = match boolean(
        input,
        "require_idempotency_key",
        Gov::RequireIdempotencyBoolean,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(layout.map(
        &["allowed", "denied", "require_idempotency_key"],
        &ids,
        charge,
    )?))
}

pub(super) fn audit<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    let Some(input) = classify_map(value, charge)? else {
        return Ok(Err(Gov::AuditShape));
    };
    let events = match boolean(
        input,
        "effect_events",
        Gov::EffectEventsBoolean,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    let denials = match enumeration(
        input,
        "denials",
        &["off", "sampled", "all"],
        Gov::Denials,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(layout.map(
        &["denials", "effect_events"],
        &[denials, events],
        charge,
    )?))
}

pub(super) fn approvals<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    let Some(input) = classify_map(value, charge)? else {
        return Ok(Err(Gov::ApprovalsShape));
    };
    let mut ids = [usize::MAX; 4];
    for (name, field, index) in [
        ("effects", Field::ApprovalEffects, 1),
        ("states", Field::ApprovalStates, 2),
        ("assignees", Field::Assignees, 0),
    ] {
        ids[index] = match super::lists::optional(input, name, field, layout, charge)? {
            Ok(id) => id,
            Err(reason) => return Ok(Err(reason)),
        };
    }
    ids[3] = match positive(
        input,
        "timeout_ms",
        Field::ApprovalTimeout,
        false,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(layout.map(
        &["assignees", "effects", "states", "timeout_ms"],
        &ids,
        charge,
    )?))
}

fn boolean<'input>(
    input: Map<'input>,
    name: &str,
    reason: Gov,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(value) = select(input, name.as_bytes(), charge)?.filter(|value| !value.is_nil())
    else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    let text = match value.atom_bytes() {
        Some(b"true") => "true",
        Some(b"false") => "false",
        _ => return Ok(Err(reason)),
    };
    Ok(Ok(layout.push_cell(Cell::Atom(text), charge)?))
}
