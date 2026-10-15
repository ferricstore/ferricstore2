//! One source-ordered top walk, compact fixed-schema staging and child-first cells.

use super::layout::{FlowPolicyCell as Cell, FlowPolicyLayout as Layout};
use super::value::{
    PolicyInputCursor as Cursor, PolicyInputMap as Map, classify_map, select, unsigned, visit,
};
use super::{Error, FlowPolicyInputRejection as Reason, PolicySchemaCharge};
use crate::policy::complete::{indexed_input, retention_input, retry_input};
use crate::policy::{Backoff, BackoffKind};

const TOP_NAMES: [&str; 9] = [
    "governance",
    "indexed_attributes",
    "indexed_state_meta",
    "max_active_ms",
    "retention",
    "retry",
    "states",
    "type",
    "version",
];

pub(super) fn normalize<'input>(
    workflow_type: Cursor<'input>,
    attrs: Cursor<'input>,
    maximum: u32,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    visit(charge, 0)?;
    let Some(workflow_type) = workflow_type.binary_bytes() else {
        return Ok(Err(Reason::RootShape));
    };
    let Some(attrs_map) = classify_map(attrs, charge)? else {
        return Ok(Err(
            if matches!(
                attrs.kind(),
                super::value::PolicyInputKind::List | super::value::PolicyInputKind::ImproperList
            ) {
                Reason::RootKeyword
            } else {
                Reason::RootShape
            },
        ));
    };
    let mut ids = [usize::MAX; 9];
    if select(attrs_map, b"mode", charge)?.is_some() {
        return Ok(Err(Reason::TopStateMode));
    }
    ids[8] = match version(select(attrs_map, b"version", charge)?, layout, charge)? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[3] = match max_active(select(attrs_map, b"max_active_ms", charge)?, layout, charge)? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[5] = match retry(select(attrs_map, b"retry", charge)?, layout, charge)? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[4] = match retention(
        select(attrs_map, b"retention", charge)?,
        maximum,
        layout,
        charge,
    )? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    if let Err(reason) = indexed(attrs_map, &mut ids, layout, charge)? {
        return Ok(Err(reason));
    }
    ids[0] = match super::governance::optional(
        select(attrs_map, b"governance", charge)?,
        layout,
        charge,
    )? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(Reason::Governance(reason))),
    };
    ids[6] = match super::states::normalize(
        select(attrs_map, b"states", charge)?,
        maximum,
        layout,
        charge,
    )? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[7] = layout.push_cell(Cell::Binary(workflow_type), charge)?;
    Ok(Ok(layout.map(&TOP_NAMES, &ids, charge)?))
}

fn version<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    visit(charge, 0)?;
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    let cell = if let Some(bytes) = value.binary_bytes().filter(|b| !b.is_empty()) {
        Cell::Binary(bytes)
    } else if let Some(number) = value.integer().and_then(|n| u128::try_from(n).ok()) {
        Cell::Integer(number)
    } else if let Some((negative, bytes)) = value.big_integer() {
        visit(charge, bytes.len())?;
        let length = bytes
            .iter()
            .rposition(|&byte| byte != 0)
            .map_or(0, |n| n + 1);
        if negative && length != 0 {
            return Ok(Err(Reason::Version));
        }
        if length <= 16 {
            let mut magnitude = [0; 16];
            magnitude[..length].copy_from_slice(&bytes[..length]);
            Cell::Integer(u128::from_le_bytes(magnitude))
        } else {
            Cell::BigInteger(&bytes[..length])
        }
    } else {
        return Ok(Err(Reason::Version));
    };
    Ok(Ok(layout.push_cell(cell, charge)?))
}

fn max_active<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    visit(charge, 0)?;
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    if value.atom_bytes() == Some(b"infinity")
        || matches!(value.binary_bytes(), Some(b"infinity" | b"INFINITY"))
    {
        return Ok(Ok(usize::MAX));
    }
    let Some(number) = unsigned(value, 31_536_000_000, charge)?.filter(|&n| n > 0) else {
        return Ok(Err(Reason::MaxActive));
    };
    Ok(Ok(
        layout.push_cell(Cell::Integer(u128::from(number)), charge)?
    ))
}

pub(super) fn retry<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    let fields = match retry_input::borrow_retry(value, false, charge)? {
        Ok(Some(fields)) => fields,
        Ok(None) => return Ok(Ok(usize::MAX)),
        Err(reason) => return Ok(Err(Reason::Retry(reason))),
    };
    let mut ids = [usize::MAX; 3];
    if let Some(backoff) = fields.backoff {
        ids[0] = stage_backoff(backoff, layout, charge)?;
    }
    if let Some(bytes) = fields.exhausted_to {
        ids[1] = layout.push_cell(Cell::Binary(bytes), charge)?;
    }
    if let Some(value) = fields.max_retries {
        ids[2] = layout.push_cell(Cell::Integer(u128::from(value)), charge)?;
    }
    Ok(Ok(layout.map(
        &["backoff", "exhausted_to", "max_retries"],
        &ids,
        charge,
    )?))
}

fn stage_backoff(
    backoff: Backoff,
    layout: &mut Layout<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let kind = match backoff.kind {
        BackoffKind::None => "none",
        BackoffKind::Fixed => "fixed",
        BackoffKind::Linear => "linear",
        BackoffKind::Exponential => "exponential",
    };
    let ids = [
        layout.push_cell(Cell::Integer(u128::from(backoff.base_ms)), charge)?,
        layout.push_cell(Cell::Integer(u128::from(backoff.jitter_pct)), charge)?,
        layout.push_cell(Cell::Atom(kind), charge)?,
        layout.push_cell(Cell::Integer(u128::from(backoff.max_ms)), charge)?,
    ];
    layout.map(&["base_ms", "jitter_pct", "kind", "max_ms"], &ids, charge)
}

pub(super) fn retention<'input>(
    selected: Option<Cursor<'input>>,
    maximum: u32,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Reason>, Error> {
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    let fields = match retention_input::borrow_retention(value, maximum, charge)? {
        Ok(Some(fields)) => fields,
        Ok(None) => return Ok(Ok(usize::MAX)),
        Err(reason) => return Ok(Err(Reason::Retention(reason))),
    };
    let mut ids = [usize::MAX; 2];
    if let Some(value) = fields.history_max_events {
        ids[0] = layout.push_cell(Cell::Integer(u128::from(value)), charge)?;
    }
    if let Some(value) = fields.ttl_ms {
        ids[1] = layout.push_cell(Cell::Integer(u128::from(value)), charge)?;
    }
    Ok(Ok(layout.map(
        &["history_max_events", "ttl_ms"],
        &ids,
        charge,
    )?))
}

fn indexed<'input>(
    map: Map<'input>,
    ids: &mut [usize; 9],
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<(), Reason>, Error> {
    let mut fields = indexed_input::BorrowedPolicyIndexed {
        count: None,
        names: [None; 3],
        state_meta: None,
    };
    fields.count = match indexed_input::borrow_attributes(
        select(map, b"indexed_attributes", charge)?,
        &mut fields.names,
        charge,
    )? {
        Ok(count) => count,
        Err(reason) => return Ok(Err(Reason::Indexed(reason))),
    };
    fields.state_meta = if let Some(value) = select(map, b"indexed_state_meta", charge)? {
        match indexed_input::borrow_metadata(value, 0, charge)? {
            Ok(name) => name,
            Err(reason) => return Ok(Err(Reason::Indexed(reason))),
        }
    } else {
        visit(charge, 0)?;
        None
    };
    if let Some(count) = fields.count {
        let first = layout.cells.len();
        for bytes in fields.names.iter().take(count).flatten() {
            layout.push_cell(Cell::Binary(bytes), charge)?;
        }
        let start = layout.edges.len();
        for index in 0..count {
            layout.push_edge(first + index, charge)?;
        }
        ids[1] = layout.push_cell(Cell::List { start, len: count }, charge)?;
    }
    if let Some(bytes) = fields.state_meta {
        ids[2] = layout.push_cell(Cell::Binary(bytes), charge)?;
    }
    Ok(Ok(()))
}
