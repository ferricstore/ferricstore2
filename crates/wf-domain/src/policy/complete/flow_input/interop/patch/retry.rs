//! Borrowed retry/backoff/retention edits without erasing unrelated raw rows.

use super::super::raw_group_parent as map;
use super::{Cell, Error, Index, Layout, PolicySchemaCharge, add, node, pay, raw, rewrite};
use crate::policy::complete::retention_input::valid_retention;
use crate::policy::complete::retry_input::{domain, invalid, number, raw_backoff_kind};
use crate::policy::definition::{BackoffPatch, FieldPatch, RetryPatch};
use crate::policy::{Backoff, BackoffKind, validate_retry};
use crate::retention::RetentionOverride;
use crate::value::RetryPolicyField;

pub(super) fn retry<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    old: Option<u32>,
    patch: &'a FieldPatch<RetryPatch>,
    depth: usize,
    history: Option<u32>,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let FieldPatch::Set(patch) = patch else {
        let old = old.ok_or(Error::Malformed)?;
        if history.is_some() {
            validate(index, old, charge)?;
        }
        return raw(index, old, depth, stage, charge);
    };
    let parent = map(index, old)?;
    let actions = [
        u8::from(patch.backoff.is_some()),
        u8::from(patch.exhausted_to.is_some()),
        u8::from(patch.max_retries.is_some()),
    ];
    rewrite(
        index,
        parent,
        (
            &["backoff", "exhausted_to", "max_retries"],
            actions,
            &[0, 1, 2],
            &[0, 1, 2],
            depth,
        ),
        stage,
        charge,
        |field, old, child_depth, index, stage, charge| match field {
            0 => backoff(
                index,
                old,
                patch.backoff,
                child_depth,
                history.is_some(),
                stage,
                charge,
            ),
            1 => {
                if let Some(value) = &patch.exhausted_to {
                    if history.is_some() {
                        validate_retry(None, None, Some(value)).map_err(domain)?;
                    }
                    return node(Cell::Binary(value), child_depth, stage, charge);
                }
                let old = old.ok_or(Error::Malformed)?;
                if history.is_some() {
                    let value = index
                        .raw_cursor(old)
                        .binary_bytes()
                        .ok_or_else(|| invalid(RetryPolicyField::ExhaustedTo))?;
                    validate_retry(None, None, Some(value)).map_err(domain)?;
                }
                raw(index, old, child_depth, stage, charge)
            }
            _ => {
                if let Some(value) = patch.max_retries {
                    if history.is_some() {
                        validate_retry(Some(value), None, None).map_err(domain)?;
                    }
                    return node(Cell::Integer(value.into()), child_depth, stage, charge);
                }
                let old = old.ok_or(Error::Malformed)?;
                if history.is_some()
                    && !index
                        .raw_cursor(old)
                        .integer()
                        .is_some_and(|n| (0..=1000).contains(&n))
                {
                    return Err(invalid(RetryPolicyField::MaxRetries));
                }
                raw(index, old, child_depth, stage, charge)
            }
        },
    )
}

fn validate(
    index: &Index<'_, 'static>,
    old: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let Some(parent) = map(index, Some(old))? else {
        return Ok(());
    };
    let values = find(
        index,
        parent,
        &["max_retries", "backoff", "exhausted_to"],
        charge,
    )?;
    let maximum = values[0]
        .map(|p| {
            number(index, p, 1000, RetryPolicyField::MaxRetries)
                .and_then(|v| u16::try_from(v).map_err(|_| invalid(RetryPolicyField::MaxRetries)))
        })
        .transpose()?;
    let backoff = values[1]
        .map(|p| backoff_value(index, Some(p), None, charge))
        .transpose()?;
    let target = values[2]
        .map(|p| {
            index
                .raw_cursor(p)
                .binary_bytes()
                .ok_or_else(|| invalid(RetryPolicyField::ExhaustedTo))
        })
        .transpose()?;
    validate_retry(maximum, backoff, target).map_err(domain)
}

fn backoff<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    old: Option<u32>,
    patch: Option<BackoffPatch>,
    depth: usize,
    validate: bool,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let Some(patch) = patch else {
        let old = old.ok_or(Error::Malformed)?;
        if validate {
            backoff_value(index, Some(old), None, charge)?
                .validate()
                .map_err(domain)?;
        }
        return raw(index, old, depth, stage, charge);
    };
    let parent = map(index, old)?;
    let values = parent
        .map(|p| {
            find(
                index,
                p,
                &["kind", "base_ms", "max_ms", "jitter_pct"],
                charge,
            )
        })
        .transpose()?
        .unwrap_or([None; 4]);
    let value = backoff_value(index, old, Some(patch), charge)?;
    if validate {
        value.validate().map_err(domain)?;
    }
    let actions = [
        u8::from(patch.kind.is_some() || values[0].is_none()),
        u8::from(patch.base_ms.is_some() || values[1].is_none()),
        u8::from(patch.max_ms.is_some() || values[2].is_none()),
        u8::from(patch.jitter_pct.is_some() || values[3].is_none()),
    ];
    rewrite(
        index,
        parent,
        (
            &["kind", "base_ms", "max_ms", "jitter_pct"],
            actions,
            &[0, 1, 2, 3],
            &[1, 3, 0, 2],
            depth,
        ),
        stage,
        charge,
        |field, old, child_depth, index, stage, charge| {
            if actions[field] == 0 {
                return raw(
                    index,
                    old.ok_or(Error::Malformed)?,
                    child_depth,
                    stage,
                    charge,
                );
            }
            let cell = match field {
                0 => Cell::Atom(match value.kind {
                    BackoffKind::None => "none",
                    BackoffKind::Fixed => "fixed",
                    BackoffKind::Linear => "linear",
                    BackoffKind::Exponential => "exponential",
                }),
                1 => Cell::Integer(value.base_ms.into()),
                2 => Cell::Integer(value.max_ms.into()),
                _ => Cell::Integer(value.jitter_pct.into()),
            };
            node(cell, child_depth, stage, charge)
        },
    )
}

fn backoff_value(
    index: &Index<'_, 'static>,
    old: Option<u32>,
    patch: Option<BackoffPatch>,
    charge: &mut PolicySchemaCharge,
) -> Result<Backoff, Error> {
    let values = map(index, old)?
        .map(|p| {
            find(
                index,
                p,
                &["kind", "base_ms", "max_ms", "jitter_pct"],
                charge,
            )
        })
        .transpose()?
        .unwrap_or([None; 4]);
    let patch = patch.unwrap_or_default();
    let defaults = Backoff::default();
    let kind = if let Some(kind) = patch.kind {
        kind
    } else if let Some(p) = values[0] {
        raw_backoff_kind(index, p, charge)?
    } else {
        defaults.kind
    };
    let base_ms = patch.base_ms.map_or_else(
        || {
            values[1].map_or(Ok(defaults.base_ms), |p| {
                number(index, p, 2_592_000_000, RetryPolicyField::BaseMillis)
            })
        },
        Ok,
    )?;
    let max_ms = patch.max_ms.map_or_else(
        || {
            values[2].map_or(Ok(defaults.max_ms), |p| {
                number(index, p, 2_592_000_000, RetryPolicyField::MaxMillis)
            })
        },
        Ok,
    )?;
    let jitter_pct = patch.jitter_pct.map_or_else(
        || {
            values[3].map_or(Ok(defaults.jitter_pct), |p| {
                number(index, p, 100, RetryPolicyField::JitterPercent).and_then(|n| {
                    u8::try_from(n).map_err(|_| invalid(RetryPolicyField::JitterPercent))
                })
            })
        },
        Ok,
    )?;
    Ok(Backoff {
        kind,
        base_ms,
        max_ms,
        jitter_pct,
    })
}

pub(super) fn retention<'raw: 'a, 'a>(
    index: &mut Index<'raw, 'static>,
    old: Option<u32>,
    patch: &FieldPatch<RetentionOverride>,
    depth: usize,
    history: Option<u32>,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let FieldPatch::Set(patch) = patch else {
        let old = old.ok_or(Error::Malformed)?;
        if let Some(maximum) = history {
            validate_retention(index, old, maximum, charge)?;
        }
        return raw(index, old, depth, stage, charge);
    };
    let parent = map(index, old)?;
    rewrite(
        index,
        parent,
        (
            &["ttl_ms", "history_max_events"],
            [
                u8::from(patch.ttl_ms.is_some()),
                u8::from(patch.history_max_events.is_some()),
            ],
            &[0, 1],
            &[1, 0],
            depth,
        ),
        stage,
        charge,
        |field, old, child_depth, index, stage, charge| {
            let set = if field == 0 {
                patch.ttl_ms
            } else {
                patch.history_max_events.map(u64::from)
            };
            if let Some(value) = set {
                if let Some(maximum) = history {
                    valid_retention(field, i128::from(value), maximum)?;
                }
                return node(Cell::Integer(value.into()), child_depth, stage, charge);
            }
            let old = old.ok_or(Error::Malformed)?;
            if let Some(maximum) = history {
                valid_retention(
                    field,
                    index.raw_cursor(old).integer().unwrap_or(-1),
                    maximum,
                )?;
            }
            raw(index, old, child_depth, stage, charge)
        },
    )
}

fn validate_retention(
    index: &Index<'_, 'static>,
    old: u32,
    maximum: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let Some(parent) = map(index, Some(old))? else {
        return Ok(());
    };
    for (field, value) in find(index, parent, &["ttl_ms", "history_max_events"], charge)?
        .into_iter()
        .enumerate()
    {
        if let Some(value) = value {
            valid_retention(
                field,
                index.raw_cursor(value).integer().unwrap_or(-1),
                maximum,
            )?;
        }
    }
    Ok(())
}
fn find<const N: usize>(
    index: &Index<'_, 'static>,
    parent: u32,
    names: &[&str; N],
    charge: &mut PolicySchemaCharge,
) -> Result<[Option<u32>; N], Error> {
    if index.raw_cursor(parent).map_id().is_none() {
        return crate::policy::complete::flow_input::value::raw_keyword_fields(
            index, parent, names, charge,
        );
    }
    let (mut atom, mut binary) = ([None; N], [None; N]);
    let mut next = parent + 1;
    while let Some((key, value, following)) = index.raw_row_positions(parent, next)? {
        next = following;
        pay(128, charge)?;
        let cursor = index.raw_cursor(key);
        let a = cursor.atom_bytes();
        let Some(bytes) = a.or_else(|| cursor.binary_bytes()) else {
            continue;
        };
        for (field, name) in names.iter().enumerate() {
            pay(add(128, add(bytes.len(), name.len())?)?, charge)?;
            if bytes != name.as_bytes() {
                continue;
            }
            if a.is_some() {
                atom[field] = Some(value);
            } else {
                binary[field] = Some(value);
            }
            break;
        }
    }
    for (slot, fallback) in atom.iter_mut().zip(binary) {
        *slot = slot.or(fallback);
    }
    Ok(atom)
}
