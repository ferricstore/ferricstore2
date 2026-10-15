//! Source map/keyword/pair classification and original per-state field order.

use super::layout::{FlowPolicyCell as Cell, FlowPolicyLayout as Layout};
use super::value::{PolicyInputCursor as Cursor, PolicyInputKind as Kind, PolicyInputMap as Map};
use super::value::{classify_map, select, visit};
use super::{Error, FlowPolicyInputRejection as Reason, PolicySchemaCharge, PolicySchemaError};

pub(super) fn normalize<'input>(
    selected: Option<Cursor<'input>>,
    maximum: u32,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    let map = match selected {
        None => Map::Empty,
        Some(value) => match classify(value, charge)? {
            Ok(map) => map,
            Err(reason) => return Ok(Err(reason)),
        },
    };
    let (start, len) = super::order::prepare_map_order(map, layout, charge)?;
    for offset in 0..len {
        let ordinal = layout.work[start + offset].index;
        let (name, input) = map.row_at(ordinal, charge)?;
        visit(charge, 0)?;
        let Some(name) = name.binary_bytes().filter(|name| !name.is_empty()) else {
            return Ok(Err(Reason::StateName));
        };
        visit(charge, name.len().checked_add(7).ok_or(Error::Malformed)?)?;
        if name == b"running" {
            return Ok(Err(Reason::RunningState));
        }
        let cell = match policy(input, maximum, layout, charge)? {
            Ok(cell) => cell,
            Err(reason) => return Ok(Err(reason)),
        };
        let name = layout.push_cell(Cell::Binary(name), charge)?;
        visit(charge, 0)?;
        let slot = &mut layout.work[start + offset];
        slot.cell = cell;
        slot.order =
            u64::try_from(name).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
    }
    let edges = layout.edges.len();
    for offset in 0..len {
        let slot = layout.work[start + offset];
        let name = usize::try_from(slot.order)
            .map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
        layout.push_edge(name, charge)?;
        layout.push_edge(slot.cell, charge)?;
    }
    let root = layout.push_cell(Cell::Map { start: edges, len }, charge)?;
    super::order::sort_output_map(layout, edges, len, charge)?;
    layout.work.truncate(start);
    Ok(Ok(root))
}

fn classify<'input>(
    input: Cursor<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Map<'input>, Reason>, Error> {
    if let Some(map) = classify_map(input, charge)? {
        return Ok(Ok(map));
    }
    if !matches!(input.kind(), Kind::List | Kind::ImproperList) {
        visit(charge, 0)?;
        return Ok(Err(Reason::StatesShape));
    }
    visit(charge, 0)?;
    let mut index = 0;
    while let Some(tuple) = input.list_item(index, charge)? {
        visit(charge, 0)?;
        if tuple.kind() != Kind::Tuple || tuple.len() != 2 {
            return Ok(Err(Reason::StatesList));
        }
        let name = tuple.child(0).ok_or(Error::Malformed)?;
        let policy = tuple.child(1).ok_or(Error::Malformed)?;
        if name.kind() != Kind::Binary
            || !matches!(policy.kind(), Kind::Map | Kind::List | Kind::ImproperList)
        {
            return Ok(Err(Reason::StatesList));
        }
        index += 1;
    }
    Ok(Ok(Map::StatePairs(input)))
}

fn policy<'input>(
    input: Cursor<'input>,
    maximum: u32,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    visit(charge, 0)?;
    let Some(map) = classify_map(input, charge)? else {
        return Ok(Err(Reason::StatePolicyShape));
    };
    if select(map, b"max_active_ms", charge)?.is_some() {
        return Ok(Err(Reason::StateMaxActive));
    }
    let mut ids = [usize::MAX; 4];
    if let Some(value) = select(map, b"mode", charge)? {
        ids[1] = match mode(value, layout, charge)? {
            Ok(id) => id,
            Err(reason) => return Ok(Err(reason)),
        };
    }
    ids[3] = match super::core::retry(select(map, b"retry", charge)?, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[2] = match super::core::retention(
        select(map, b"retention", charge)?,
        maximum,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[0] = match super::governance::optional(select(map, b"governance", charge)?, layout, charge)?
    {
        Ok(id) => id,
        Err(reason) => return Ok(Err(Reason::Governance(reason))),
    };
    Ok(Ok(layout.map(
        &["governance", "mode", "retention", "retry"],
        &ids,
        charge,
    )?))
}

fn mode<'input>(
    value: Cursor<'input>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    visit(charge, 0)?;
    let Some(bytes) = value.key_bytes() else {
        return Ok(Err(Reason::StateMode));
    };
    for token in ["parallel", "fifo"] {
        visit(
            charge,
            bytes
                .len()
                .checked_add(token.len())
                .ok_or(Error::Malformed)?,
        )?;
        if if value.is_atom() {
            bytes == token.as_bytes()
        } else {
            bytes.eq_ignore_ascii_case(token.as_bytes())
        } {
            return Ok(Ok(layout.push_cell(Cell::Atom(token), charge)?));
        }
    }
    Ok(Err(Reason::StateMode))
}
