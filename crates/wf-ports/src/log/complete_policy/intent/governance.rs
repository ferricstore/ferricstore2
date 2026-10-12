//! Ordered governance fields in the generation-free Complete Replace intent.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, CompletePolicyRequestError, DenialAuditMode,
    EffectsData, GovernanceData, GovernanceMode, LimitData,
};
use wf_domain::workflow::active::LimitEnforcement;

use super::{CompletePolicyIntentSink, write_names};

pub(super) fn write_governance<W: CompletePolicyIntentSink>(
    value: Option<&GovernanceData>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(value.is_some())?;
    let Some(value) = value else {
        return Ok(());
    };
    writer.optional(value.mode.is_some())?;
    if let Some(mode) = value.mode {
        writer.u8(match mode {
            GovernanceMode::Minimal => 0,
            GovernanceMode::Ledger => 1,
            GovernanceMode::Full => 2,
        })?;
    }
    write_effects(value.effects.as_ref(), writer)?;
    write_audit(value.audit.as_ref(), writer)?;
    write_limits(value.limits.as_ref(), writer)?;
    write_budgets(value.budgets.as_ref(), writer)?;
    write_circuits(value.circuits.as_ref(), writer)?;
    write_approvals(value.approvals.as_ref(), writer)
}

fn write_effects<W: CompletePolicyIntentSink>(
    value: Option<&EffectsData>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(value.is_some())?;
    let Some(value) = value else {
        return Ok(());
    };
    write_names(value.allowed.as_deref(), writer)?;
    write_names(value.denied.as_deref(), writer)?;
    writer.optional(value.require_idempotency_key.is_some())?;
    if let Some(flag) = value.require_idempotency_key {
        writer.u8(u8::from(flag))?;
    }
    Ok(())
}

fn write_audit<W: CompletePolicyIntentSink>(
    value: Option<&AuditData>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(value.is_some())?;
    let Some(value) = value else {
        return Ok(());
    };
    writer.optional(value.effect_events.is_some())?;
    if let Some(flag) = value.effect_events {
        writer.u8(u8::from(flag))?;
    }
    writer.optional(value.denials.is_some())?;
    if let Some(mode) = value.denials {
        writer.u8(match mode {
            DenialAuditMode::Off => 0,
            DenialAuditMode::Sampled => 1,
            DenialAuditMode::All => 2,
        })?;
    }
    Ok(())
}

fn write_limits<W: CompletePolicyIntentSink>(
    values: Option<&BTreeMap<Vec<u8>, LimitData>>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(values.is_some())?;
    let Some(values) = values else {
        return Ok(());
    };
    writer.count(values.len())?;
    for (key, value) in values {
        writer.raw(key)?;
        write_limit(value, writer)?;
    }
    Ok(())
}

fn write_budgets<W: CompletePolicyIntentSink>(
    values: Option<&BTreeMap<Vec<u8>, BudgetData>>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(values.is_some())?;
    let Some(values) = values else {
        return Ok(());
    };
    writer.count(values.len())?;
    for (key, value) in values {
        writer.raw(key)?;
        write_budget(value, writer)?;
    }
    Ok(())
}

fn write_circuits<W: CompletePolicyIntentSink>(
    values: Option<&BTreeMap<Vec<u8>, CircuitData>>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(values.is_some())?;
    let Some(values) = values else {
        return Ok(());
    };
    writer.count(values.len())?;
    for (key, value) in values {
        writer.raw(key)?;
        write_circuit(value, writer)?;
    }
    Ok(())
}

fn write_limit<W: CompletePolicyIntentSink>(
    value: &LimitData,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.u64(value.limit)?;
    write_enforcement(value.enforcement, writer)?;
    writer.optional(value.lease_size.is_some())?;
    if let Some(lease) = value.lease_size {
        writer.u64(lease)?;
    }
    Ok(())
}

fn write_budget<W: CompletePolicyIntentSink>(
    value: &BudgetData,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.u64(value.limit)?;
    writer.u64(value.window_ms)?;
    write_enforcement(value.enforcement, writer)?;
    writer.optional(value.lease_size.is_some())?;
    if let Some(lease) = value.lease_size {
        writer.u64(lease)?;
    }
    writer.optional(value.unit.is_some())?;
    if let Some(unit) = &value.unit {
        writer.raw(unit)?;
    }
    Ok(())
}

fn write_circuit<W: CompletePolicyIntentSink>(
    value: &CircuitData,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.u64(value.failure_threshold)?;
    writer.u64(value.open_ms)?;
    writer.optional(value.window_ms.is_some())?;
    if let Some(scalar) = value.window_ms {
        writer.u64(scalar)?;
    }
    writer.optional(value.min_calls.is_some())?;
    if let Some(scalar) = value.min_calls {
        writer.u64(scalar)?;
    }
    writer.optional(value.failure_rate_pct.is_some())?;
    if let Some(scalar) = value.failure_rate_pct {
        writer.u8(scalar)?;
    }
    writer.optional(value.latency_threshold_ms.is_some())?;
    if let Some(scalar) = value.latency_threshold_ms {
        writer.u64(scalar)?;
    }
    writer.optional(value.half_open_max_probes.is_some())?;
    if let Some(scalar) = value.half_open_max_probes {
        writer.u64(scalar)?;
    }
    writer.optional(value.half_open_success_threshold.is_some())?;
    if let Some(scalar) = value.half_open_success_threshold {
        writer.u64(scalar)?;
    }
    write_names(value.error_classes.as_deref(), writer)
}

fn write_approvals<W: CompletePolicyIntentSink>(
    value: Option<&ApprovalsData>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(value.is_some())?;
    let Some(value) = value else {
        return Ok(());
    };
    write_names(value.effects.as_deref(), writer)?;
    write_names(value.states.as_deref(), writer)?;
    write_names(value.assignees.as_deref(), writer)?;
    writer.optional(value.timeout_ms.is_some())?;
    if let Some(timeout) = value.timeout_ms {
        writer.u64(timeout)?;
    }
    Ok(())
}

fn write_enforcement<W: CompletePolicyIntentSink>(
    value: Option<LimitEnforcement>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(value.is_some())?;
    if let Some(value) = value {
        writer.u8(match value {
            LimitEnforcement::StrictGlobal => 0,
            LimitEnforcement::ApproximateGlobal => 1,
        })?;
    }
    Ok(())
}
