//! Raw state deletion, partial configuration edits and final cardinality.

use super::{
    Cell, Error, Index, Layout, PolicyError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, Position, action, add, container, edge, node, pay, policy, raw, rewrite,
};
use crate::policy::definition::{FieldPatch, StateChange, StateMode, StatePolicyPatch};
use crate::value::{DomainError, RetryPolicyField};
use std::collections::BTreeMap;

pub(super) fn states<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    old: Option<u32>,
    changes: &'a BTreeMap<Vec<u8>, StateChange>,
    context: (usize, Option<u32>, PolicySchemaLimits),
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let (depth, history, schema) = context;
    let parent = map(index, old)?;
    let building = stage.0.is_some();
    let expected = if building {
        parent.map_or(
            Ok(changes
                .values()
                .filter(|v| matches!(v, StateChange::Patch(_)))
                .count()),
            |p| index.splice_map_len(Position::Encoded(p)),
        )?
    } else {
        0
    };
    let (root, start, mut length) = container(false, expected, depth, stage, charge)?;
    let (mut count, mut matched) = (0, 0);
    if let Some(parent) = parent {
        let mut next = parent + 1;
        while let Some((key, value, following)) = index.raw_row_positions(parent, next)? {
            next = following;
            pay(128, charge)?;
            let name = index.raw_cursor(key).binary_bytes();
            let mut update = None;
            if let Some(name) = name {
                for (wanted, candidate) in changes {
                    pay(add(128, add(name.len(), wanted.len())?)?, charge)?;
                    if wanted == name {
                        update = Some(candidate);
                        matched += 1;
                        break;
                    }
                }
            }
            if matches!(update, Some(StateChange::Remove)) {
                continue;
            }
            if history.is_some() {
                valid_state(name.ok_or_else(|| policy(PolicyError::InvalidStatePolicy))?)?;
            }
            let key = raw(index, key, depth + 1, stage, charge)?;
            let value = config(
                index,
                Some(value),
                update.and_then(|v| match v {
                    StateChange::Patch(p) => Some(p),
                    StateChange::Remove => None,
                }),
                (depth + 1, history),
                stage,
                charge,
            )?;
            edge(add(start, count * 2)?, key.0, stage)?;
            edge(add(start, count * 2 + 1)?, value.0, stage)?;
            length = add(length, add(key.1, value.1)?)?;
            count = add(count, 1)?;
        }
    }
    if matched != changes.len() {
        for (name, update) in changes {
            if parent
                .map(|p| exists(index, p, name, charge))
                .transpose()?
                .unwrap_or(false)
            {
                continue;
            }
            let StateChange::Patch(update) = update else {
                continue;
            };
            let key = node(Cell::Binary(name), depth + 1, stage, charge)?;
            let value = config(
                index,
                None,
                Some(update),
                (depth + 1, history),
                stage,
                charge,
            )?;
            edge(add(start, count * 2)?, key.0, stage)?;
            edge(add(start, count * 2 + 1)?, value.0, stage)?;
            length = add(length, add(key.1, value.1)?)?;
            count = add(count, 1)?;
        }
    }
    if !building {
        for _ in 0..count.checked_mul(2).ok_or_else(super::overflow)? {
            stage.1 = add(stage.1, 1)?;
            pay(128, charge)?;
        }
        if let Some(parent) = parent {
            index.record_splice_map_len(Position::Encoded(parent), count)?;
        }
    } else if count != expected {
        return Err(Error::EncodingMismatch);
    }
    if history.is_some() && count > schema.policy_limits().max_states() {
        return Err(policy(PolicyError::StateBudget));
    }
    Ok((root, length))
}

fn exists(
    index: &Index<'_, 'static>,
    parent: u32,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let map = index.raw_cursor(parent);
    let (mut low, mut high) = (0, map.len());
    while low < high {
        let mid = low + (high - low) / 2;
        pay(128, charge)?;
        let Some((key, _)) = map.row(mid) else {
            return Err(Error::Malformed);
        };
        let Some(bytes) = key.binary_bytes() else {
            low = mid + 1;
            continue;
        };
        pay(add(128, add(bytes.len(), name.len())?)?, charge)?;
        match bytes.cmp(name) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Ok(true),
        }
    }
    Ok(false)
}

fn map(index: &Index<'_, 'static>, old: Option<u32>) -> Result<Option<u32>, Error> {
    old.map(|p| {
        index
            .raw_cursor(p)
            .map_id()
            .ok_or_else(|| policy(PolicyError::InvalidStatePolicy))
    })
    .transpose()
}

fn config<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    old: Option<u32>,
    update: Option<&'a StatePolicyPatch>,
    context: (usize, Option<u32>),
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let (depth, history) = context;
    let parent = old.map(|p| {
        let value = index.raw_cursor(p);
        value.map_id().or_else(|| matches!(value.kind(), crate::policy::complete::flow_input::value::PolicyInputKind::List | crate::policy::complete::flow_input::value::PolicyInputKind::ImproperList).then_some(p))
            .ok_or_else(|| policy(PolicyError::InvalidStatePolicy))
    }).transpose()?;
    let mode = update.map_or(&FieldPatch::Unchanged, |p| &p.mode);
    let retry = update.map_or(&FieldPatch::Unchanged, |p| &p.retry);
    let retention = update.map_or(&FieldPatch::Unchanged, |p| &p.retention);
    rewrite(
        index,
        parent,
        (
            &["mode", "retry", "retention"],
            [action(mode), action(retry), action(retention)],
            &[0, 1, 2],
            &[0, 2, 1],
            depth,
        ),
        stage,
        charge,
        |field, old, child_depth, index, stage, charge| match field {
            0 => {
                if let FieldPatch::Set(mode) = mode {
                    return node(
                        Cell::Atom(match mode {
                            StateMode::Fifo => "fifo",
                            StateMode::Parallel => "parallel",
                        }),
                        child_depth,
                        stage,
                        charge,
                    );
                }
                let old = old.ok_or(Error::Malformed)?;
                if history.is_some() && !valid_mode(index, old, charge)? {
                    return Err(policy(PolicyError::InvalidStateMode));
                }
                raw(index, old, child_depth, stage, charge)
            }
            1 => super::retry::retry(index, old, retry, child_depth, history, stage, charge),
            _ => {
                super::retry::retention(index, old, retention, child_depth, history, stage, charge)
            }
        },
    )
}

fn valid_mode(
    index: &Index<'_, 'static>,
    id: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let value = index.raw_cursor(id);
    if let Some(atom) = value.atom_bytes() {
        return Ok(matches!(atom, b"parallel" | b"fifo"));
    }
    let Some(bytes) = value.binary_bytes() else {
        return Ok(false);
    };
    pay(128, charge)?;
    for token in [b"parallel".as_slice(), b"fifo"] {
        pay(add(128, add(bytes.len(), token.len())?)?, charge)?;
        if bytes.eq_ignore_ascii_case(token) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn valid_state(name: &[u8]) -> Result<(), Error> {
    if name.is_empty() || name == b"running" {
        return Err(policy(PolicyError::Domain(
            DomainError::InvalidRetryPolicy(if name.is_empty() {
                RetryPolicyField::State
            } else {
                RetryPolicyField::RunningState
            }),
        )));
    }
    if name.len() > 262_144 {
        return Err(Error::Schema(PolicySchemaError::SnapshotBytes));
    }
    Ok(())
}
