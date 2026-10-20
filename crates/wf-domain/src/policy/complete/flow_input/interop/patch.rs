//! Internal raw Core stages; original siblings and scalar encodings stay literal.

use super::{
    Cell, Error, Layout, PolicySchemaCharge, PolicySchemaLimits, add, bound, container, edge,
    envelope_length, node, overflow, pay, peak,
};
use crate::policy::complete::original_entry::{
    TermPosition as Position, indexed::OriginalPolicyCurrentIndex as Index,
};
use crate::policy::complete::{CompleteCorePolicyPatchInput as Input, PolicySchemaError};
use crate::policy::definition::{FieldPatch, PolicyPatch, PolicyVersion, StateChange};
use crate::policy::installation::PolicyError;
mod retry;
mod states;
use states::valid_state;

pub(in crate::policy::complete) fn admit_raw(
    input: &Input,
    _schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    for _ in 0..5 {
        pay(128, charge)?;
    }
    if let FieldPatch::Set(v) = &input.patch.version {
        validate_version(v)?;
    }
    if let FieldPatch::Set(v) = input.patch.max_active_ms
        && (v == 0 || v > crate::retention::MAX_RETENTION_MILLIS)
    {
        return Err(policy(PolicyError::InvalidMaxActive));
    }
    for (name, change) in &input.patch.states {
        pay(128, charge)?;
        pay(add(128, name.len())?, charge)?;
        if let StateChange::Patch(patch) = change {
            valid_state(name)?;
            for _ in 0..3 {
                pay(128, charge)?;
            }
            if matches!(patch.mode, FieldPatch::Clear) {
                return Err(policy(PolicyError::InvalidStateMode));
            }
        }
    }
    Ok(())
}

pub(in crate::policy::complete) fn measure_raw(
    index: &mut Index<'_, 'static>,
    input: &Input,
    history: Option<u32>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize, usize), Error> {
    if history.is_some_and(|h| h == 0 || h > crate::retention::MAX_HISTORY_EVENTS) {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    let mut stage = (None, 0, 0);
    let (_, snapshot) = root(index, &input.patch, (history, schema), &mut stage, charge)?;
    pay(1, charge)?;
    Ok((add(1, snapshot)?, stage.1, stage.2))
}

pub(in crate::policy::complete) fn encode_raw<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    input: &'a Input,
    generation: u64,
    measured: (usize, usize, usize),
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    additional_live: usize,
) -> Result<Vec<u8>, Error> {
    let (snapshot, count, depth) = measured;
    let length = envelope_length(snapshot, generation)?;
    if length > 263_168 || snapshot > 262_144 {
        return Err(Error::ByteLimit);
    }
    let live = add(additional_live, index.owner_bound()?)?;
    let layout = Layout::new_with_base(count, depth, live, charge)?;
    let mut stage = (Some(layout), 0, 0);
    let (root, built) = root(index, &input.patch, (None, schema), &mut stage, charge)?;
    if add(1, built)? != snapshot {
        return Err(Error::EncodingMismatch);
    }
    peak(
        add(add(live, bound(count, depth)?)?, add(512, length)?)?,
        charge,
    )?;
    super::super::etf::emit_envelope(
        stage.0.as_ref().ok_or(Error::Malformed)?,
        root,
        snapshot,
        generation,
        charge,
    )
}

fn root<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    patch: &'a PolicyPatch,
    context: (Option<u32>, PolicySchemaLimits),
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let parent = index.raw_policy_position();
    let (history, schema) = context;
    let actions = [
        action(&patch.version),
        action(&patch.max_active_ms),
        action(&patch.retry),
        action(&patch.retention),
        u8::from(!patch.states.is_empty()),
    ];
    rewrite(
        index,
        Some(parent),
        (
            &["version", "max_active_ms", "retry", "retention", "states"],
            actions,
            &[2, 3, 4],
            &[1, 3, 2, 4, 0],
            0,
        ),
        stage,
        charge,
        |field, old, child_depth, index, stage, charge| match field {
            0 => scalar_version(
                index,
                old,
                &patch.version,
                child_depth,
                history.is_some(),
                stage,
                charge,
            ),
            1 => scalar_active(
                index,
                old,
                &patch.max_active_ms,
                child_depth,
                history.is_some(),
                stage,
                charge,
            ),
            2 => retry::retry(
                index,
                old,
                &patch.retry,
                child_depth,
                history,
                stage,
                charge,
            ),
            3 => retry::retention(
                index,
                old,
                &patch.retention,
                child_depth,
                history,
                stage,
                charge,
            ),
            _ => states::states(
                index,
                old,
                &patch.states,
                (child_depth, history, schema),
                stage,
                charge,
            ),
        },
    )
}

pub(super) fn rewrite<'raw: 'a, 'a, const N: usize>(
    index: &mut Index<'raw, 'static>,
    parent: Option<u32>,
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
    if let Some(parent) = parent
        && index.raw_cursor(parent).map_id().is_none()
    {
        return super::super::value::rewrite_keyword(index, parent, schema, stage, charge, value);
    }
    let (names, actions, queries, append_order, depth) = schema;
    if N > 9 {
        return Err(Error::Malformed);
    }
    let building = stage.0.is_some();
    let expected = if building {
        parent.map_or(
            Ok(actions
                .iter()
                .fold(0_usize, |count, &value| count + usize::from(value == 1))),
            |p| index.splice_map_len(Position::Encoded(p)),
        )?
    } else {
        0
    };
    let (root, start, mut length) = container(false, expected, depth, stage, charge)?;
    let mut rows = [None; 18];
    let mut count = 0;
    if let Some(parent) = parent {
        let mut next = parent + 1;
        while let Some((key, child, following)) = index.raw_row_positions(parent, next)? {
            next = following;
            pay(128, charge)?;
            if let Some((field, atom)) = selected(index, key, names, queries, charge)? {
                if actions[field] == 2 {
                    continue;
                }
                rows[field * 2 + usize::from(!atom)] = Some((key, child, count));
            } else {
                let k = raw(index, key, depth + 1, stage, charge)?;
                let v = raw(index, child, depth + 1, stage, charge)?;
                edge(add(start, count * 2)?, k.0, stage)?;
                edge(add(start, count * 2 + 1)?, v.0, stage)?;
                length = add(length, add(k.1, v.1)?)?;
            }
            count = add(count, 1)?;
        }
    }
    let mut appended = [super::absent(); N];
    for field in 0..N {
        let selected = rows[field * 2].or(rows[field * 2 + 1]);
        for row in [rows[field * 2], rows[field * 2 + 1]].into_iter().flatten() {
            let k = raw(index, row.0, depth + 1, stage, charge)?;
            let v = if selected.is_some_and(|s| s.0 == row.0) {
                value(field, Some(row.1), depth + 1, index, stage, charge)?
            } else {
                raw(index, row.1, depth + 1, stage, charge)?
            };
            edge(add(start, row.2 * 2)?, k.0, stage)?;
            edge(add(start, row.2 * 2 + 1)?, v.0, stage)?;
            length = add(length, add(k.1, v.1)?)?;
        }
        if selected.is_none() && actions[field] == 1 {
            appended[field] = value(field, None, depth + 1, index, stage, charge)?;
        }
    }
    for &field in append_order {
        if appended[field].0 != usize::MAX {
            let k = node(Cell::Atom(names[field]), depth + 1, stage, charge)?;
            let v = appended[field];
            edge(add(start, count * 2)?, k.0, stage)?;
            edge(add(start, count * 2 + 1)?, v.0, stage)?;
            length = add(length, add(k.1, v.1)?)?;
            count = add(count, 1)?;
        }
    }
    if !building {
        for _ in 0..count.checked_mul(2).ok_or_else(overflow)? {
            stage.1 = add(stage.1, 1)?;
            pay(128, charge)?;
        }
        if let Some(parent) = parent {
            index.record_splice_map_len(Position::Encoded(parent), count)?;
        }
    } else if count != expected {
        return Err(Error::EncodingMismatch);
    }
    Ok((root, length))
}

pub(in crate::policy::complete::flow_input) fn selected<const N: usize>(
    index: &Index<'_, 'static>,
    key: u32,
    names: &[&str; N],
    queries: &[usize],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(usize, bool)>, Error> {
    let cursor = index.raw_cursor(key);
    let atom = cursor.atom_bytes();
    let Some(bytes) = atom.or_else(|| cursor.binary_bytes()) else {
        return Ok(None);
    };
    for &field in queries {
        pay(add(128, add(bytes.len(), names[field].len())?)?, charge)?;
        if bytes == names[field].as_bytes() {
            return Ok(Some((field, atom.is_some())));
        }
    }
    if N == 5 {
        return Ok(match bytes {
            b"version" => Some((0, atom.is_some())),
            b"max_active_ms" => Some((1, atom.is_some())),
            _ => None,
        });
    }
    Ok(None)
}

pub(in crate::policy::complete::flow_input) fn raw<'raw: 'a, 'a>(
    index: &Index<'raw, 'static>,
    id: u32,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let actual_depth = add(depth, index.raw_depth(id, charge)?)?;
    node(
        Cell::RawTerm(index.raw_span(id)?),
        actual_depth,
        stage,
        charge,
    )
}
pub(super) fn action<T>(value: &FieldPatch<T>) -> u8 {
    match value {
        FieldPatch::Unchanged => 0,
        FieldPatch::Set(_) => 1,
        FieldPatch::Clear => 2,
    }
}
pub(super) fn policy(error: PolicyError) -> Error {
    Error::Schema(PolicySchemaError::Policy(error))
}
fn validate_version(value: &PolicyVersion) -> Result<(), Error> {
    if matches!(value, PolicyVersion::Bytes(b) if b.is_empty()) {
        return Err(policy(PolicyError::InvalidVersion));
    }
    if matches!(value, PolicyVersion::Bytes(b) if b.len() > 262_144) {
        return Err(Error::Schema(PolicySchemaError::SnapshotBytes));
    }
    Ok(())
}
fn scalar_version<'raw: 'a, 'a>(
    index: &Index<'raw, 'static>,
    old: Option<u32>,
    patch: &'a FieldPatch<PolicyVersion>,
    depth: usize,
    validate: bool,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    if let FieldPatch::Set(v) = patch {
        return node(
            match v {
                PolicyVersion::Bytes(b) => Cell::Binary(b),
                PolicyVersion::Number(n) => Cell::Integer((*n).into()),
            },
            depth,
            stage,
            charge,
        );
    }
    let old = old.ok_or(Error::Malformed)?;
    let v = index.raw_cursor(old);
    if validate
        && v.atom_bytes().is_none_or(|b| b != b"nil")
        && v.binary_bytes().is_none_or(<[u8]>::is_empty)
        && v.integer().is_none_or(|n| n < 0)
        && !v
            .big_integer()
            .is_some_and(|(negative, b)| !negative || b.iter().all(|&v| v == 0))
    {
        return Err(policy(PolicyError::InvalidVersion));
    }
    raw(index, old, depth, stage, charge)
}
fn scalar_active<'raw: 'a, 'a>(
    index: &Index<'raw, 'static>,
    old: Option<u32>,
    patch: &FieldPatch<u64>,
    depth: usize,
    validate: bool,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    if let FieldPatch::Set(v) = patch {
        return node(Cell::Integer((*v).into()), depth, stage, charge);
    }
    let old = old.ok_or(Error::Malformed)?;
    let v = index.raw_cursor(old);
    if validate
        && v.atom_bytes() != Some(b"nil")
        && !v
            .integer()
            .is_some_and(|n| (1..=31_536_000_000).contains(&n))
        && !matches!(v.binary_bytes(), Some(b"infinity" | b"INFINITY"))
    {
        return Err(policy(PolicyError::InvalidMaxActive));
    }
    raw(index, old, depth, stage, charge)
}
