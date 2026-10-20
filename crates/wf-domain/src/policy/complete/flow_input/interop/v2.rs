//! Complete typed fields, preserving presence and every opaque leaf.

use super::{
    Cell, Error, Layout, PolicySchemaCharge, absent, add, container, edge, fields, node, strings,
};
use crate::policy::complete::{CompletePolicyData, CompleteStatePolicyData};
use crate::policy::definition::{PolicyVersion, StateMode};
use crate::policy::{Backoff, BackoffKind, RetryOverride};
use crate::retention::RetentionOverride;
mod governance;

pub(super) fn policy<'a>(
    data: &'a CompletePolicyData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut values = [absent(); 9];
    if let Some(value) = &data.governance {
        values[0] = governance::policy(value, d, stage, charge)?;
    }
    if let Some(value) = &data.indexed_attributes {
        values[1] = strings(value, d, stage, charge)?;
    }
    if let Some(value) = &data.indexed_state_meta {
        values[2] = node(Cell::Binary(value), d, stage, charge)?;
    }
    if let Some(value) = data.max_active_ms {
        values[3] = node(Cell::Integer(value.into()), d, stage, charge)?;
    }
    if let Some(value) = &data.retention {
        values[4] = retention(value, d, stage, charge)?;
    }
    if let Some(value) = &data.retry {
        values[5] = retry(value, d, stage, charge)?;
    }
    let (root, start, mut length) = container(false, data.states.len(), d, stage, charge)?;
    for (ordinal, (name, value)) in data.states.iter().enumerate() {
        let key = node(Cell::Binary(name), add(d, 1)?, stage, charge)?;
        let value = state(value, add(d, 1)?, stage, charge)?;
        edge(add(start, ordinal * 2)?, key.0, stage)?;
        edge(add(start, ordinal * 2 + 1)?, value.0, stage)?;
        length = add(length, add(key.1, value.1)?)?;
    }
    values[6] = (root, length);
    values[7] = node(Cell::Binary(&data.workflow_type), d, stage, charge)?;
    if let Some(version) = &data.version {
        values[8] = node(
            match version {
                PolicyVersion::Bytes(b) => Cell::Binary(b),
                PolicyVersion::Number(n) => Cell::Integer((*n).into()),
            },
            d,
            stage,
            charge,
        )?;
    }
    fields(
        &[
            "governance",
            "indexed_attributes",
            "indexed_state_meta",
            "max_active_ms",
            "retention",
            "retry",
            "states",
            "type",
            "version",
        ],
        &values,
        depth,
        stage,
        charge,
    )
}

fn state<'a>(
    value: &'a CompleteStatePolicyData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut values = [absent(); 4];
    if let Some(v) = &value.governance {
        values[0] = governance::policy(v, d, stage, charge)?;
    }
    if let Some(v) = value.mode {
        values[1] = node(
            Cell::Atom(match v {
                StateMode::Parallel => "parallel",
                StateMode::Fifo => "fifo",
            }),
            d,
            stage,
            charge,
        )?;
    }
    if let Some(v) = &value.retention {
        values[2] = retention(v, d, stage, charge)?;
    }
    if let Some(v) = &value.retry {
        values[3] = retry(v, d, stage, charge)?;
    }
    fields(
        &["governance", "mode", "retention", "retry"],
        &values,
        depth,
        stage,
        charge,
    )
}

pub(in super::super) fn retry<'a>(
    value: &'a RetryOverride,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    borrowed_retry(
        value.max_retries,
        value.backoff,
        value.exhausted_to.as_deref(),
        depth,
        stage,
        charge,
    )
}

pub(in super::super) fn borrowed_retry<'a>(
    maximum: Option<u16>,
    backoff: Option<Backoff>,
    target: Option<&'a [u8]>,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut values = [absent(); 3];
    if let Some(v) = backoff {
        values[0] = stage_backoff(v, d, stage, charge)?;
    }
    if let Some(v) = target {
        values[1] = node(Cell::Binary(v), d, stage, charge)?;
    }
    if let Some(v) = maximum {
        values[2] = node(Cell::Integer(v.into()), d, stage, charge)?;
    }
    fields(
        &["backoff", "exhausted_to", "max_retries"],
        &values,
        depth,
        stage,
        charge,
    )
}

pub(in super::super) fn stage_backoff(
    value: Backoff,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let values = [
        node(Cell::Integer(value.base_ms.into()), d, stage, charge)?,
        node(Cell::Integer(value.jitter_pct.into()), d, stage, charge)?,
        node(
            Cell::Atom(match value.kind {
                BackoffKind::None => "none",
                BackoffKind::Fixed => "fixed",
                BackoffKind::Linear => "linear",
                BackoffKind::Exponential => "exponential",
            }),
            d,
            stage,
            charge,
        )?,
        node(Cell::Integer(value.max_ms.into()), d, stage, charge)?,
    ];
    fields(
        &["base_ms", "jitter_pct", "kind", "max_ms"],
        &values,
        depth,
        stage,
        charge,
    )
}

pub(in super::super) fn retention(
    value: &RetentionOverride,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut values = [absent(); 2];
    if let Some(v) = value.history_max_events {
        values[0] = node(Cell::Integer(v.into()), d, stage, charge)?;
    }
    if let Some(v) = value.ttl_ms {
        values[1] = node(Cell::Integer(v.into()), d, stage, charge)?;
    }
    fields(
        &["history_max_events", "ttl_ms"],
        &values,
        depth,
        stage,
        charge,
    )
}
