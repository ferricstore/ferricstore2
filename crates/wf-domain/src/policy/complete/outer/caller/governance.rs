//! Paid normalized governance fields in their fixed intent order.

use std::collections::BTreeMap;

use crate::workflow::active::LimitEnforcement;

use super::super::super::{
    ApprovalsData, AuditData, BudgetData, CircuitData, DenialAuditMode, EffectsData,
    GovernanceData, GovernanceMode, LimitData,
};
use super::super::CompletePolicyRequestError;
use super::measure::IntentMeasure;

pub(super) fn measure_governance(
    data: &GovernanceData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(data.mode.is_some())?;
    if let Some(mode) = data.mode {
        measure.u8(match mode {
            GovernanceMode::Minimal => 0,
            GovernanceMode::Ledger => 1,
            GovernanceMode::Full => 2,
        })?;
    }
    measure.optional(data.effects.is_some())?;
    if let Some(effects) = &data.effects {
        measure_effects(effects, measure)?;
    }
    measure.optional(data.audit.is_some())?;
    if let Some(audit) = data.audit {
        measure_audit(audit, measure)?;
    }
    measure.optional(data.limits.is_some())?;
    if let Some(limits) = &data.limits {
        measure_limits(limits, measure)?;
    }
    measure.optional(data.budgets.is_some())?;
    if let Some(budgets) = &data.budgets {
        measure_budgets(budgets, measure)?;
    }
    measure.optional(data.circuits.is_some())?;
    if let Some(circuits) = &data.circuits {
        measure_circuits(circuits, measure)?;
    }
    measure.optional(data.approvals.is_some())?;
    if let Some(approvals) = &data.approvals {
        measure_approvals(approvals, measure)?;
    }
    Ok(())
}

fn measure_effects(
    data: &EffectsData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(data.allowed.is_some())?;
    if let Some(names) = &data.allowed {
        super::measure_names(names, 1_000, 65_535, measure)?;
    }
    measure.optional(data.denied.is_some())?;
    if let Some(names) = &data.denied {
        super::measure_names(names, 1_000, 65_535, measure)?;
    }
    measure.optional(data.require_idempotency_key.is_some())?;
    if let Some(required) = data.require_idempotency_key {
        measure.u8(u8::from(required))?;
    }
    Ok(())
}

fn measure_audit(
    data: AuditData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(data.effect_events.is_some())?;
    if let Some(events) = data.effect_events {
        measure.u8(u8::from(events))?;
    }
    measure.optional(data.denials.is_some())?;
    if let Some(denials) = data.denials {
        measure.u8(match denials {
            DenialAuditMode::Off => 0,
            DenialAuditMode::Sampled => 1,
            DenialAuditMode::All => 2,
        })?;
    }
    Ok(())
}

fn measure_limits(
    data: &BTreeMap<Vec<u8>, LimitData>,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.count(data.len(), 1_000)?;
    for (name, rule) in data {
        measure.node()?;
        measure.raw(name, 65_535)?;
        measure_limit(*rule, measure)?;
    }
    Ok(())
}

fn measure_budgets(
    data: &BTreeMap<Vec<u8>, BudgetData>,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.count(data.len(), 1_000)?;
    for (name, rule) in data {
        measure.node()?;
        measure.raw(name, 65_535)?;
        measure_budget(rule, measure)?;
    }
    Ok(())
}

fn measure_circuits(
    data: &BTreeMap<Vec<u8>, CircuitData>,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.count(data.len(), 1_000)?;
    for (name, rule) in data {
        measure.node()?;
        measure.raw(name, 65_535)?;
        measure_circuit(rule, measure)?;
    }
    Ok(())
}

fn measure_limit(
    data: LimitData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.u64(data.limit)?;
    measure.optional(data.enforcement.is_some())?;
    if let Some(enforcement) = data.enforcement {
        measure_enforcement(enforcement, measure)?;
    }
    measure.optional(data.lease_size.is_some())?;
    if let Some(lease) = data.lease_size {
        measure.u64(lease)?;
    }
    Ok(())
}

fn measure_budget(
    data: &BudgetData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.u64(data.limit)?;
    measure.u64(data.window_ms)?;
    measure.optional(data.enforcement.is_some())?;
    if let Some(enforcement) = data.enforcement {
        measure_enforcement(enforcement, measure)?;
    }
    measure.optional(data.lease_size.is_some())?;
    if let Some(lease) = data.lease_size {
        measure.u64(lease)?;
    }
    measure.optional(data.unit.is_some())?;
    if let Some(unit) = &data.unit {
        measure.raw(unit, 65_535)?;
    }
    Ok(())
}

fn measure_circuit(
    data: &CircuitData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.u64(data.failure_threshold)?;
    measure.u64(data.open_ms)?;
    for value in [data.window_ms, data.min_calls] {
        measure.optional(value.is_some())?;
        if let Some(value) = value {
            measure.u64(value)?;
        }
    }
    measure.optional(data.failure_rate_pct.is_some())?;
    if let Some(rate) = data.failure_rate_pct {
        measure.u8(rate)?;
    }
    for value in [
        data.latency_threshold_ms,
        data.half_open_max_probes,
        data.half_open_success_threshold,
    ] {
        measure.optional(value.is_some())?;
        if let Some(value) = value {
            measure.u64(value)?;
        }
    }
    measure.optional(data.error_classes.is_some())?;
    if let Some(names) = &data.error_classes {
        super::measure_names(names, 1_000, 256, measure)?;
    }
    Ok(())
}

fn measure_approvals(
    data: &ApprovalsData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    for names in [
        data.effects.as_ref(),
        data.states.as_ref(),
        data.assignees.as_ref(),
    ] {
        measure.optional(names.is_some())?;
        if let Some(names) = names {
            super::measure_names(names, 1_000, 65_535, measure)?;
        }
    }
    measure.optional(data.timeout_ms.is_some())?;
    if let Some(timeout) = data.timeout_ms {
        measure.u64(timeout)?;
    }
    Ok(())
}

fn measure_enforcement(
    value: LimitEnforcement,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.u8(match value {
        LimitEnforcement::StrictGlobal => 0,
        LimitEnforcement::ApproximateGlobal => 1,
    })
}
