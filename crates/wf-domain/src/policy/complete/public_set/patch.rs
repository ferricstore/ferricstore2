//! Source `PolicyPatch` conversion without business normalization or tuple walks.

use super::{
    Error, PolicySchemaCharge, PolicySetPatchArena, PolicySetPatchCell, PolicySetPatchSlot, Raw,
    atom, authority, debit, is_list, item, row,
};

mod rows;

pub(super) fn normalize<'input>(
    opts: Raw<'input>,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let (start, len) = rows::keyword(opts, true, work, charge)?;
    debit(charge, 128)?;
    staged(opts, true, start, len, arena, work, charge)
}

fn value<'input>(
    input: Raw<'input>,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    debit(charge, 128)?;
    match input {
        Raw::Map(_) => direct(input, arena, work, charge),
        Raw::List([]) => arena.cell(
            PolicySetPatchCell::List {
                start: arena.edges.len(),
                len: 0,
            },
            charge,
        ),
        _ if is_list(input) => {
            if keyword(input, charge)? {
                let (start, len) = rows::keyword(input, false, work, charge)?;
                debit(charge, 128)?;
                staged(input, false, start, len, arena, work, charge)
            } else {
                list(input, arena, work, charge)
            }
        }
        _ => arena.cell(PolicySetPatchCell::Raw(input), charge),
    }
}

fn keyword(input: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<bool, Error> {
    let mut index = 0;
    loop {
        let (entry, proper) = item(input, index, charge)?;
        let Some(Raw::Tuple(pair)) = entry else {
            return Ok(entry.is_none() && proper);
        };
        if pair.len() != 2 || atom(pair[0]).is_none() {
            return Ok(false);
        }
        index += 1;
    }
}

fn states<'input>(
    input: Raw<'input>,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    debit(charge, 128)?;
    if matches!(input, Raw::Map(_)) {
        return direct(input, arena, work, charge);
    }
    if is_list(input) && state_pairs(input, charge)? {
        debit(charge, 128)?;
        let start = work.len();
        let mut index = 0;
        loop {
            let (entry, proper) = item(input, index, charge)?;
            let Some(Raw::Tuple(pair)) = entry else {
                if entry.is_some() || !proper {
                    return Err(Error::Malformed);
                }
                break;
            };
            let Raw::Binary(name) = pair[0] else {
                return Err(Error::Malformed);
            };
            let cell = value(pair[1], arena, work, charge)?;
            let hash = rows::fingerprint(name, charge)?;
            rows::push(
                work,
                PolicySetPatchSlot {
                    source: index,
                    cell,
                    hash,
                },
                charge,
            )?;
            index += 1;
        }
        let len = work.len() - start;
        let len = rows::compact(input, work, start, len, charge)?;
        return assemble(input, start, len, false, arena, work, charge);
    }
    value(input, arena, work, charge)
}

fn state_pairs(input: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<bool, Error> {
    let mut index = 0;
    loop {
        let (entry, proper) = item(input, index, charge)?;
        let Some(Raw::Tuple(pair)) = entry else {
            return match entry {
                None if proper => Ok(true),
                None => Err(authority()),
                Some(_) => Ok(false),
            };
        };
        if pair.len() != 2
            || !matches!(pair[0], Raw::Binary(_))
            || !(matches!(pair[1], Raw::Map(_)) || is_list(pair[1]))
        {
            return Ok(false);
        }
        index += 1;
    }
}

fn staged<'input>(
    input: Raw<'input>,
    top: bool,
    start: usize,
    len: usize,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    for slot in &work[start..start + len] {
        let (key, input) = row(input, slot.source, charge)?;
        struct_member(key, input, charge)?;
    }
    for index in start..start + len {
        let (key, input) = row(input, work[index].source, charge)?;
        let special = if top {
            let bytes = match key {
                Raw::Binary(bytes) => bytes,
                _ => atom(key).ok_or(Error::Malformed)?,
            };
            debit(charge, 128 + bytes.len() + 6)?;
            bytes == b"states"
        } else {
            false
        };
        let cell = if special {
            states(input, arena, work, charge)?
        } else {
            value(input, arena, work, charge)?
        };
        debit(charge, 128)?;
        work[index].cell = cell;
    }
    assemble(input, start, len, false, arena, work, charge)
}

fn direct<'input>(
    input: Raw<'input>,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let Raw::Map(entries) = input else {
        return Err(Error::Malformed);
    };
    for index in 0..entries.len() {
        let (key, input) = row(input, index, charge)?;
        struct_member(key, input, charge)?;
    }
    let start = work.len();
    let reverse = entries.len() > 32;
    for offset in 0..entries.len() {
        let index = if reverse {
            entries.len() - 1 - offset
        } else {
            offset
        };
        let (_, input) = row(input, index, charge)?;
        let cell = value(input, arena, work, charge)?;
        rows::push(
            work,
            PolicySetPatchSlot {
                source: index,
                cell,
                hash: 0,
            },
            charge,
        )?;
    }
    assemble(input, start, entries.len(), reverse, arena, work, charge)
}

fn struct_member(
    key: Raw<'_>,
    input: Raw<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    if let Some(bytes) = atom(key) {
        debit(charge, 128 + bytes.len() + 10)?;
        if bytes == b"__struct__" && atom(input).is_some() {
            return Err(authority());
        }
    }
    Ok(())
}

fn assemble<'input>(
    input: Raw<'input>,
    start: usize,
    len: usize,
    reverse: bool,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let edges = arena.edges.len();
    for offset in 0..len {
        let index = start + if reverse { len - 1 - offset } else { offset };
        let slot = work[index];
        let (key, _) = row(input, slot.source, charge)?;
        let key = arena.cell(PolicySetPatchCell::Raw(key), charge)?;
        arena.edge(key, charge)?;
        arena.edge(slot.cell, charge)?;
    }
    let result = arena.cell(
        PolicySetPatchCell::Map {
            start: edges,
            pairs: len,
        },
        charge,
    )?;
    work.truncate(start);
    Ok(result)
}

fn list<'input>(
    input: Raw<'input>,
    arena: &mut PolicySetPatchArena<'input>,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let start = work.len();
    let mut index = 0;
    loop {
        let (input, proper) = item(input, index, charge)?;
        let Some(input) = input else {
            if !proper {
                return Err(authority());
            }
            break;
        };
        let cell = value(input, arena, work, charge)?;
        rows::push(
            work,
            PolicySetPatchSlot {
                source: index,
                cell,
                hash: 0,
            },
            charge,
        )?;
        index += 1;
    }
    let edges = arena.edges.len();
    for slot in &work[start..] {
        arena.edge(slot.cell, charge)?;
    }
    let result = arena.cell(
        PolicySetPatchCell::List {
            start: edges,
            len: index,
        },
        charge,
    )?;
    work.truncate(start);
    Ok(result)
}
