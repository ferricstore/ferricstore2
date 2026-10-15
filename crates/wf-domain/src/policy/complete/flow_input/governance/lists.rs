//! Raw-occurrence count and exact-on-hit first-seen list deduplication.

use super::super::layout::FlowPolicyWorkSlot;
use super::super::value::PolicyInputKind as Kind;
use super::{Cell, Cursor, Error, Field, Gov, Layout, Map, PolicySchemaCharge, select, visit};

pub(super) fn optional<'input>(
    map: Map<'input>,
    name: &str,
    field: Field,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(input) = select(map, name.as_bytes(), charge)?.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    if !matches!(input.kind(), Kind::List | Kind::ImproperList) {
        return Ok(Err(Gov::StringList(field)));
    }
    let start = layout.work.len();
    let mut chunk = input;
    let mut count = 0;
    loop {
        if !matches!(chunk.kind(), Kind::List | Kind::ImproperList) {
            return Ok(Err(Gov::ListValue));
        }
        for index in 0..chunk.len() {
            visit(charge, 0)?;
            if count == 1_000 {
                return Ok(Err(Gov::ListCount));
            }
            let child = chunk.child(index).ok_or(Error::Malformed)?;
            let Some(bytes) = child.binary_bytes().filter(|bytes| !bytes.is_empty()) else {
                return Ok(Err(Gov::ListValue));
            };
            if bytes.len()
                > if field == Field::ErrorClasses {
                    256
                } else {
                    65_535
                }
            {
                return Ok(Err(Gov::ListBytes(field)));
            }
            let hash = super::super::order::hash::binary_hash(bytes, charge)?;
            if !duplicate(input, bytes, hash, start, layout, charge)? {
                layout.push_work(
                    FlowPolicyWorkSlot {
                        hash,
                        index: count,
                        cell: 0,
                        order: 0,
                    },
                    charge,
                )?;
            }
            count += 1;
        }
        let Some(tail) = chunk.tail() else {
            break;
        };
        visit(charge, 0)?;
        chunk = tail;
    }
    let length = layout.work.len() - start;
    let first_cell = layout.cells.len();
    for index in start..layout.work.len() {
        let ordinal = layout.work[index].index;
        let bytes = input
            .list_item(ordinal, charge)?
            .and_then(Cursor::binary_bytes)
            .ok_or(Error::Malformed)?;
        layout.push_cell(Cell::Binary(bytes), charge)?;
    }
    let edges = layout.edges.len();
    for index in 0..length {
        layout.push_edge(first_cell + index, charge)?;
    }
    let root = layout.push_cell(
        Cell::List {
            start: edges,
            len: length,
        },
        charge,
    )?;
    layout.work.truncate(start);
    Ok(Ok(root))
}

fn duplicate(
    input: Cursor<'_>,
    bytes: &[u8],
    hash: u64,
    start: usize,
    layout: &Layout<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    for slot in &layout.work[start..] {
        super::super::super::charge_schema(charge, 16, 0).map_err(Error::Schema)?;
        if slot.hash == hash {
            let previous = input
                .list_item(slot.index, charge)?
                .and_then(Cursor::binary_bytes)
                .ok_or(Error::Malformed)?;
            visit(
                charge,
                bytes
                    .len()
                    .checked_add(previous.len())
                    .ok_or(Error::Malformed)?,
            )?;
            if bytes == previous {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
