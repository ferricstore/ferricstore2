//! Canonical full governance rendering; no resolution or incoming normalization.

use super::{
    Cell, Error, Layout, PolicySchemaCharge, absent, add, container, edge, fields, node, strings,
};
use crate::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, DenialAuditMode, EffectsData,
    GovernanceData, GovernanceMode, LimitData,
};
use crate::workflow::active::LimitEnforcement;
use std::collections::BTreeMap;

pub(super) fn policy<'a>(
    value: &'a GovernanceData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut v = [absent(); 7];
    if let Some(x) = &value.approvals {
        v[0] = approvals(x, d, stage, charge)?;
    }
    if let Some(x) = &value.audit {
        v[1] = audit(*x, d, stage, charge)?;
    }
    if let Some(x) = &value.budgets {
        v[2] = rules(x, d, stage, charge, budget)?;
    }
    if let Some(x) = &value.circuits {
        v[3] = rules(x, d, stage, charge, circuit)?;
    }
    if let Some(x) = &value.effects {
        v[4] = effects(x, d, stage, charge)?;
    }
    if let Some(x) = &value.limits {
        v[5] = rules(x, d, stage, charge, limit)?;
    }
    if let Some(x) = value.mode {
        v[6] = node(
            Cell::Atom(match x {
                GovernanceMode::Minimal => "minimal",
                GovernanceMode::Ledger => "ledger",
                GovernanceMode::Full => "full",
            }),
            d,
            stage,
            charge,
        )?;
    }
    fields(
        &[
            "approvals",
            "audit",
            "budgets",
            "circuits",
            "effects",
            "limits",
            "mode",
        ],
        &v,
        depth,
        stage,
        charge,
    )
}

fn rules<'a, T>(
    value: &'a BTreeMap<Vec<u8>, T>,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
    render: impl Fn(
        &'a T,
        usize,
        &mut (Option<Layout<'a>>, usize, usize),
        &mut PolicySchemaCharge,
    ) -> Result<(usize, usize), Error>,
) -> Result<(usize, usize), Error> {
    let (root, start, mut length) = container(false, value.len(), depth, stage, charge)?;
    for (ordinal, (name, value)) in value.iter().enumerate() {
        let key = node(Cell::Binary(name), add(depth, 1)?, stage, charge)?;
        let value = render(value, add(depth, 1)?, stage, charge)?;
        edge(add(start, ordinal * 2)?, key.0, stage)?;
        edge(add(start, ordinal * 2 + 1)?, value.0, stage)?;
        length = add(length, add(key.1, value.1)?)?;
    }
    Ok((root, length))
}

fn uint(
    value: Option<u64>,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    value.map_or(Ok(absent()), |n| {
        node(Cell::Integer(n.into()), depth, stage, charge)
    })
}
fn boolean(
    value: Option<bool>,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    value.map_or(Ok(absent()), |v| {
        node(
            Cell::Atom(if v { "true" } else { "false" }),
            depth,
            stage,
            charge,
        )
    })
}
fn enforcement(
    value: Option<LimitEnforcement>,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    value.map_or(Ok(absent()), |v| {
        node(
            Cell::Atom(match v {
                LimitEnforcement::StrictGlobal => "strict_global",
                LimitEnforcement::ApproximateGlobal => "approximate_global",
            }),
            depth,
            stage,
            charge,
        )
    })
}

fn limit(
    value: &LimitData,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let v = [
        enforcement(value.enforcement, d, stage, charge)?,
        uint(value.lease_size, d, stage, charge)?,
        uint(Some(value.limit), d, stage, charge)?,
    ];
    fields(
        &["enforcement", "lease_size", "limit"],
        &v,
        depth,
        stage,
        charge,
    )
}
fn budget<'a>(
    value: &'a BudgetData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let v = [
        enforcement(value.enforcement, d, stage, charge)?,
        uint(value.lease_size, d, stage, charge)?,
        uint(Some(value.limit), d, stage, charge)?,
        value
            .unit
            .as_ref()
            .map_or(Ok(absent()), |b| node(Cell::Binary(b), d, stage, charge))?,
        uint(Some(value.window_ms), d, stage, charge)?,
    ];
    fields(
        &["enforcement", "lease_size", "limit", "unit", "window_ms"],
        &v,
        depth,
        stage,
        charge,
    )
}
fn circuit<'a>(
    value: &'a CircuitData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut v = [absent(); 9];
    if let Some(x) = &value.error_classes {
        v[0] = strings(x, d, stage, charge)?;
    }
    for (slot, n) in [
        (1, value.failure_rate_pct.map(u64::from)),
        (2, Some(value.failure_threshold)),
        (3, value.half_open_max_probes),
        (4, value.half_open_success_threshold),
        (5, value.latency_threshold_ms),
        (6, value.min_calls),
        (7, Some(value.open_ms)),
        (8, value.window_ms),
    ] {
        v[slot] = uint(n, d, stage, charge)?;
    }
    fields(
        &[
            "error_classes",
            "failure_rate_pct",
            "failure_threshold",
            "half_open_max_probes",
            "half_open_success_threshold",
            "latency_threshold_ms",
            "min_calls",
            "open_ms",
            "window_ms",
        ],
        &v,
        depth,
        stage,
        charge,
    )
}
fn effects<'a>(
    value: &'a EffectsData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut v = [absent(); 3];
    if let Some(x) = &value.allowed {
        v[0] = strings(x, d, stage, charge)?;
    }
    if let Some(x) = &value.denied {
        v[1] = strings(x, d, stage, charge)?;
    }
    v[2] = boolean(value.require_idempotency_key, d, stage, charge)?;
    fields(
        &["allowed", "denied", "require_idempotency_key"],
        &v,
        depth,
        stage,
        charge,
    )
}
fn audit(
    value: AuditData,
    depth: usize,
    stage: &mut (Option<Layout<'_>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let denials = value.denials.map_or(Ok(absent()), |v| {
        node(
            Cell::Atom(match v {
                DenialAuditMode::Off => "off",
                DenialAuditMode::Sampled => "sampled",
                DenialAuditMode::All => "all",
            }),
            d,
            stage,
            charge,
        )
    })?;
    let v = [denials, boolean(value.effect_events, d, stage, charge)?];
    fields(&["denials", "effect_events"], &v, depth, stage, charge)
}
fn approvals<'a>(
    value: &'a ApprovalsData,
    depth: usize,
    stage: &mut (Option<Layout<'a>>, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let d = add(depth, 1)?;
    let mut v = [absent(); 4];
    for (slot, names) in [
        (0, &value.assignees),
        (1, &value.effects),
        (2, &value.states),
    ] {
        if let Some(names) = names {
            v[slot] = strings(names, d, stage, charge)?;
        }
    }
    v[3] = uint(value.timeout_ms, d, stage, charge)?;
    fields(
        &["assignees", "effects", "states", "timeout_ms"],
        &v,
        depth,
        stage,
        charge,
    )
}
