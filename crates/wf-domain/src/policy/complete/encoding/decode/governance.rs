//! Known governance owners materialized only after their full wire preflight.

use std::collections::BTreeMap;

use crate::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, DenialAuditMode, EffectsData,
    GovernanceData, GovernanceMode, LimitData,
};
use crate::workflow::active::LimitEnforcement;

use super::super::{CompletePolicyEncodingError, Reader};
use super::{copy_leaf, read_names};

pub(super) fn read_governance(
    reader: &mut Reader<'_, '_>,
) -> Result<GovernanceData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    let mode = reader.optional(|reader| match reader.byte()? {
        0 => Ok(GovernanceMode::Minimal),
        1 => Ok(GovernanceMode::Ledger),
        2 => Ok(GovernanceMode::Full),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    let effects = reader.optional(read_effects)?;
    let audit = reader.optional(read_audit)?;
    let limits = reader.optional(read_limits)?;
    let budgets = reader.optional(read_budgets)?;
    let circuits = reader.optional(read_circuits)?;
    let approvals = reader.optional(read_approvals)?;
    Ok(GovernanceData {
        mode,
        effects,
        audit,
        limits,
        budgets,
        circuits,
        approvals,
    })
}

fn read_effects(reader: &mut Reader<'_, '_>) -> Result<EffectsData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    Ok(EffectsData {
        allowed: reader.optional(|reader| read_names(reader, 1000, 65535))?,
        denied: reader.optional(|reader| read_names(reader, 1000, 65535))?,
        require_idempotency_key: reader.optional(|reader| match reader.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CompletePolicyEncodingError::UnsupportedTag),
        })?,
    })
}

fn read_audit(reader: &mut Reader<'_, '_>) -> Result<AuditData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    let effect_events = reader.optional(|reader| match reader.byte()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    let denials = reader.optional(|reader| match reader.byte()? {
        0 => Ok(DenialAuditMode::Off),
        1 => Ok(DenialAuditMode::Sampled),
        2 => Ok(DenialAuditMode::All),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    Ok(AuditData {
        effect_events,
        denials,
    })
}

fn read_limits(
    reader: &mut Reader<'_, '_>,
) -> Result<BTreeMap<Vec<u8>, LimitData>, CompletePolicyEncodingError> {
    let count = reader.count(1000)?;
    reader.node()?;
    let mut values = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        reader.node()?;
        let raw = reader.raw(65535)?;
        if let Some(previous) = previous {
            reader.keys(previous, raw, 65535)?;
        }
        previous = Some(raw);
        let key = copy_leaf(raw, reader)?;
        let value = read_limit(reader)?;
        values.insert(key, value);
    }
    Ok(values)
}

fn read_budgets(
    reader: &mut Reader<'_, '_>,
) -> Result<BTreeMap<Vec<u8>, BudgetData>, CompletePolicyEncodingError> {
    let count = reader.count(1000)?;
    reader.node()?;
    let mut values = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        reader.node()?;
        let raw = reader.raw(65535)?;
        if let Some(previous) = previous {
            reader.keys(previous, raw, 65535)?;
        }
        previous = Some(raw);
        let key = copy_leaf(raw, reader)?;
        let value = read_budget(reader)?;
        values.insert(key, value);
    }
    Ok(values)
}

fn read_circuits(
    reader: &mut Reader<'_, '_>,
) -> Result<BTreeMap<Vec<u8>, CircuitData>, CompletePolicyEncodingError> {
    let count = reader.count(1000)?;
    reader.node()?;
    let mut values = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        reader.node()?;
        let raw = reader.raw(65535)?;
        if let Some(previous) = previous {
            reader.keys(previous, raw, 65535)?;
        }
        previous = Some(raw);
        let key = copy_leaf(raw, reader)?;
        let value = read_circuit(reader)?;
        values.insert(key, value);
    }
    Ok(values)
}

fn read_limit(reader: &mut Reader<'_, '_>) -> Result<LimitData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    Ok(LimitData {
        limit: reader.u64()?,
        enforcement: reader.optional(read_enforcement)?,
        lease_size: reader.optional(Reader::u64)?,
    })
}

fn read_budget(reader: &mut Reader<'_, '_>) -> Result<BudgetData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    let limit = reader.u64()?;
    let window_ms = reader.u64()?;
    let enforcement = reader.optional(read_enforcement)?;
    let lease_size = reader.optional(Reader::u64)?;
    let unit = reader.optional(|reader| {
        let bytes = reader.raw(65535)?;
        copy_leaf(bytes, reader)
    })?;
    Ok(BudgetData {
        limit,
        window_ms,
        enforcement,
        lease_size,
        unit,
    })
}

fn read_circuit(reader: &mut Reader<'_, '_>) -> Result<CircuitData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    Ok(CircuitData {
        failure_threshold: reader.u64()?,
        open_ms: reader.u64()?,
        window_ms: reader.optional(Reader::u64)?,
        min_calls: reader.optional(Reader::u64)?,
        failure_rate_pct: reader.optional(Reader::byte)?,
        latency_threshold_ms: reader.optional(Reader::u64)?,
        half_open_max_probes: reader.optional(Reader::u64)?,
        half_open_success_threshold: reader.optional(Reader::u64)?,
        error_classes: reader.optional(|reader| read_names(reader, 1000, 256))?,
    })
}

fn read_approvals(
    reader: &mut Reader<'_, '_>,
) -> Result<ApprovalsData, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    Ok(ApprovalsData {
        effects: reader.optional(|reader| read_names(reader, 1000, 65535))?,
        states: reader.optional(|reader| read_names(reader, 1000, 65535))?,
        assignees: reader.optional(|reader| read_names(reader, 1000, 65535))?,
        timeout_ms: reader.optional(Reader::u64)?,
    })
}

fn read_enforcement(
    reader: &mut Reader<'_, '_>,
) -> Result<LimitEnforcement, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => Ok(LimitEnforcement::StrictGlobal),
        1 => Ok(LimitEnforcement::ApproximateGlobal),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}
