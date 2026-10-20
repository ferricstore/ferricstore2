//! Existing source-map adapters, kept separate from cursor scalar operations.

use super::super::interop::{add, container, edge, node, pay, raw, selected};
use super::super::layout::{FlowPolicyCell as Cell, FlowPolicyLayout as Layout};
use super::{
    Error, PolicyInputCursor, PolicyInputKind, PolicySchemaCharge, PolicySchemaError, overflow,
    visit,
};
use crate::policy::complete::original_entry::{
    TermPosition, indexed::OriginalPolicyCurrentIndex as Index,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::policy::complete) enum PolicyInputMap<'input> {
    Direct(PolicyInputCursor<'input>),
    Keyword(PolicyInputCursor<'input>),
    StatePairs(PolicyInputCursor<'input>),
    Empty,
}

impl<'input> PolicyInputMap<'input> {
    pub(in crate::policy::complete) fn bind_source_order(
        self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        match self {
            Self::Direct(PolicyInputCursor::Merge(value)) => Ok(Self::Direct(
                PolicyInputCursor::Merge(value.bind_source_order(charge)?),
            )),
            Self::Direct(PolicyInputCursor::Frame4(value)) => {
                // This new adapter's sidecar is already source ordered.
                for index in 0..value.len() {
                    visit(charge, 0)?;
                    let (key, child) = value.row(index).ok_or(Error::Malformed)?;
                    if let Some(bytes) = key.atom_bytes() {
                        visit(charge, bytes.len().checked_add(10).ok_or_else(overflow)?)?;
                        if bytes == b"__struct__" && child.atom_bytes().is_some() {
                            return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
                        }
                    }
                }
                Ok(self)
            }
            _ => Ok(self),
        }
    }

    pub(in crate::policy::complete) fn needs_native_order(self) -> bool {
        matches!(self, Self::Direct(PolicyInputCursor::Merge(value)) if value.needs_native_order())
    }

    pub(in crate::policy::complete) fn source_row_at(
        &self,
        index: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(usize, PolicyInputCursor<'input>, PolicyInputCursor<'input>), Error> {
        if let Self::Direct(PolicyInputCursor::Merge(value)) = *self {
            let (logical, key, child) = value.source_row(index).ok_or(Error::Malformed)?;
            return Ok((
                logical,
                PolicyInputCursor::Merge(key),
                PolicyInputCursor::Merge(child),
            ));
        }
        let (key, child) = self.row_at(index, charge)?;
        Ok((index, key, child))
    }

    pub(in crate::policy::complete) fn len(
        &self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        match *self {
            Self::Empty => Ok(0),
            Self::Direct(c) => Ok(c.len()),
            Self::Keyword(c) if c.kind() == PolicyInputKind::Keyword => Ok(c.len()),
            Self::Keyword(mut c) | Self::StatePairs(mut c) => {
                let mut length = c.len();
                while let Some(tail) = c.tail() {
                    visit(charge, 0)?;
                    c = tail;
                    if !matches!(
                        c.kind(),
                        PolicyInputKind::List | PolicyInputKind::ImproperList
                    ) {
                        return Err(Error::Malformed);
                    }
                    length = length.checked_add(c.len()).ok_or_else(overflow)?;
                }
                Ok(length)
            }
        }
    }
    pub(in crate::policy::complete) fn row_at(
        &self,
        index: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(PolicyInputCursor<'input>, PolicyInputCursor<'input>), Error> {
        match *self {
            Self::Empty => Err(Error::Malformed),
            Self::Direct(c) => c.row(index).ok_or(Error::Malformed),
            Self::Keyword(c) if c.kind() == PolicyInputKind::Keyword => {
                c.row(index).ok_or(Error::Malformed)
            }
            Self::Keyword(c) | Self::StatePairs(c) => {
                let tuple = c.list_item(index, charge)?.ok_or(Error::Malformed)?;
                Ok((
                    tuple.child(0).ok_or(Error::Malformed)?,
                    tuple.child(1).ok_or(Error::Malformed)?,
                ))
            }
        }
    }
}

pub(in crate::policy::complete::flow_input) fn raw_keyword_fields<const N: usize>(
    index: &Index<'_, 'static>,
    parent: u32,
    names: &[&str; N],
    charge: &mut PolicySchemaCharge,
) -> Result<[Option<u32>; N], Error> {
    let head = index.raw_keyword_start(parent, charge)?;
    let mut cursor = (head, head.checked_add(1).ok_or_else(overflow)?);
    let mut fields = [None; N];
    while let Some((_, key, value)) = index.raw_keyword_pair(&mut cursor, charge)? {
        let bytes = index.raw_cursor(key).atom_bytes().ok_or(Error::Malformed)?;
        for (field, name) in names.iter().enumerate() {
            pay(add(128, add(bytes.len(), name.len())?)?, charge)?;
            if bytes == name.as_bytes() {
                fields[field] = Some(value);
                break;
            }
        }
    }
    Ok(fields)
}

pub(in crate::policy::complete::flow_input) fn rewrite_keyword<'raw: 'a, 'a, const N: usize>(
    index: &mut Index<'raw, 'static>,
    parent: u32,
    schema: (&[&'static str; N], [u8; N], &[usize], &[usize], usize),
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
    mut value: impl FnMut(
        usize,
        Option<u32>,
        usize,
        &mut Index<'raw, 'static>,
        &mut (Option<Layout<'a>>, usize, usize),
        &mut PolicySchemaCharge,
    ) -> Result<(usize, usize), Error>,
) -> Result<(usize, usize), Error> {
    let (names, actions, queries, append_order, depth) = schema;
    if N > 9 {
        return Err(Error::Malformed);
    }
    if actions.iter().all(|&action| action == 0) {
        let fields = raw_keyword_fields(index, parent, names, charge)?;
        for (field, old) in fields.into_iter().enumerate() {
            if let Some(old) = old {
                value(
                    field,
                    Some(old),
                    depth + 2,
                    index,
                    &mut (None, 0, 0),
                    charge,
                )?;
            }
        }
        return raw(index, parent, depth, stage, charge);
    }
    let head = index.raw_keyword_start(parent, charge)?;
    let nonempty = index.raw_cursor(head).len() != 0;
    let building = stage.0.is_some();
    let expected = keyword_expected(index, head, &actions, building, nonempty)?;
    let (root, start, mut length) = container(true, expected, depth, stage, charge)?;
    let mut rows: [Option<(u32, u32, u32, usize)>; N] = [None; N];
    let mut cursor = (head, head.checked_add(1).ok_or_else(overflow)?);
    let mut count = 0;
    while let Some((pair, key, child)) = index.raw_keyword_pair(&mut cursor, charge)? {
        if let Some((field, _)) = selected(index, key, names, queries, charge)? {
            if actions[field] == 2 {
                continue;
            }
            if let Some(previous) = rows[field].replace((pair, key, child, count)) {
                length = add(
                    length,
                    raw_pair(index, previous.0, previous.3, start, depth, stage, charge)?,
                )?;
            }
        } else {
            length = add(
                length,
                raw_pair(index, pair, count, start, depth, stage, charge)?,
            )?;
        }
        count = add(count, 1)?;
    }
    let mut appended = [(usize::MAX, 0); N];
    for field in 0..N {
        if let Some((tuple, key, child, ordinal)) = rows[field] {
            let pair = if actions[field] == 0 {
                value(
                    field,
                    Some(child),
                    depth + 2,
                    index,
                    &mut (None, 0, 0),
                    charge,
                )?;
                raw(index, tuple, depth + 1, stage, charge)?
            } else {
                let key = raw(index, key, depth + 2, stage, charge)?;
                let child = value(field, Some(child), depth + 2, index, stage, charge)?;
                keyword_pair(key, child, depth + 1, stage, charge)?
            };
            edge(add(start, ordinal)?, pair.0, stage)?;
            length = add(length, pair.1)?;
        } else if actions[field] == 1 {
            appended[field] = value(field, None, depth + 2, index, stage, charge)?;
        }
    }
    for &field in append_order {
        if appended[field].0 != usize::MAX {
            let key = node(Cell::Atom(names[field]), depth + 2, stage, charge)?;
            let pair = keyword_pair(key, appended[field], depth + 1, stage, charge)?;
            edge(add(start, count)?, pair.0, stage)?;
            length = add(length, pair.1)?;
            count = add(count, 1)?;
        }
    }
    if !building {
        if count != 0 {
            length = add(length, 5)?;
            pay(5, charge)?;
        }
        for _ in 0..count {
            stage.1 = add(stage.1, 1)?;
            pay(128, charge)?;
        }
        if nonempty {
            index.record_splice_keyword_len(TermPosition::Encoded(head), count)?;
        }
    } else if count != expected {
        return Err(Error::EncodingMismatch);
    }
    Ok((root, length))
}

fn raw_pair<'raw: 'a, 'a>(
    index: &Index<'raw, 'static>,
    id: u32,
    ordinal: usize,
    start: usize,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let pair = raw(index, id, depth + 1, stage, charge)?;
    edge(add(start, ordinal)?, pair.0, stage)?;
    Ok(pair.1)
}

fn keyword_pair(
    key: (usize, usize),
    value: (usize, usize),
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let start = stage.0.as_ref().map_or(0, |layout| layout.edges.len());
    let (root, header) = node(Cell::Tuple { start, len: 2 }, depth, stage, charge)?;
    for child in [key.0, value.0] {
        if let Some(layout) = stage.0.as_mut() {
            layout.push_edge(child, charge)?;
        } else {
            stage.1 = add(stage.1, 1)?;
            pay(128, charge)?;
        }
    }
    Ok((root, add(header, add(key.1, value.1)?)?))
}

pub(in crate::policy::complete::flow_input) fn raw_group_parent(
    index: &Index<'_, 'static>,
    old: Option<u32>,
) -> Result<Option<u32>, Error> {
    let Some(old) = old else {
        return Ok(None);
    };
    let value = index.raw_cursor(old);
    if value.atom_bytes() == Some(b"nil") {
        return Ok(None);
    }
    if let Some(parent) = value.map_id() {
        return Ok(Some(parent));
    }
    if matches!(
        value.kind(),
        crate::policy::complete::flow_input::value::PolicyInputKind::List
            | crate::policy::complete::flow_input::value::PolicyInputKind::ImproperList
    ) {
        return Ok(Some(old));
    }
    Err(Error::Schema(PolicySchemaError::InvalidAuthority))
}

fn keyword_expected(
    index: &Index<'_, 'static>,
    head: u32,
    actions: &[u8],
    building: bool,
    nonempty: bool,
) -> Result<usize, Error> {
    Ok(if building && nonempty {
        index.splice_keyword_len(TermPosition::Encoded(head))?
    } else if building {
        actions
            .iter()
            .fold(0_usize, |count, &a| count + usize::from(a == 1))
    } else {
        0
    })
}
