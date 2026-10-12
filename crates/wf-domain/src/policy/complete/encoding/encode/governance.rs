//! Known governance fields retain option presence and their original list order.

use std::collections::BTreeMap;

use crate::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, DenialAuditMode, EffectsData,
    GovernanceData, GovernanceMode, LimitData,
};
use crate::workflow::active::LimitEnforcement;

use super::super::{CompletePolicyEncodingError, Writer};
use super::write_names;

pub(super) fn write_governance(
    value: &GovernanceData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.optional(value.mode.as_ref(), |mode, writer| {
        writer.byte(match mode {
            GovernanceMode::Minimal => 0,
            GovernanceMode::Ledger => 1,
            GovernanceMode::Full => 2,
        })
    })?;
    writer.optional(value.effects.as_ref(), write_effects)?;
    writer.optional(value.audit.as_ref(), |value, writer| {
        write_audit(*value, writer)
    })?;
    writer.optional(value.limits.as_ref(), write_limits)?;
    writer.optional(value.budgets.as_ref(), write_budgets)?;
    writer.optional(value.circuits.as_ref(), write_circuits)?;
    writer.optional(value.approvals.as_ref(), write_approvals)
}

fn write_effects(
    value: &EffectsData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.optional(value.allowed.as_ref(), |names, writer| {
        write_names(names, 1000, 65535, writer)
    })?;
    writer.optional(value.denied.as_ref(), |names, writer| {
        write_names(names, 1000, 65535, writer)
    })?;
    writer.optional(value.require_idempotency_key.as_ref(), |value, writer| {
        writer.byte(u8::from(*value))
    })
}

fn write_audit(
    value: AuditData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.optional(value.effect_events.as_ref(), |value, writer| {
        writer.byte(u8::from(*value))
    })?;
    writer.optional(value.denials.as_ref(), |mode, writer| {
        writer.byte(match mode {
            DenialAuditMode::Off => 0,
            DenialAuditMode::Sampled => 1,
            DenialAuditMode::All => 2,
        })
    })
}

fn write_limits(
    values: &BTreeMap<Vec<u8>, LimitData>,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.count(values.len(), 1000)?;
    for (key, value) in values {
        writer.node()?;
        writer.raw(key, 65535)?;
        write_limit(*value, writer)?;
    }
    Ok(())
}

fn write_budgets(
    values: &BTreeMap<Vec<u8>, BudgetData>,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.count(values.len(), 1000)?;
    for (key, value) in values {
        writer.node()?;
        writer.raw(key, 65535)?;
        write_budget(value, writer)?;
    }
    Ok(())
}

fn write_circuits(
    values: &BTreeMap<Vec<u8>, CircuitData>,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.count(values.len(), 1000)?;
    for (key, value) in values {
        writer.node()?;
        writer.raw(key, 65535)?;
        write_circuit(value, writer)?;
    }
    Ok(())
}

fn write_limit(
    value: LimitData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.u64(value.limit)?;
    writer.optional(value.enforcement.as_ref(), |value, writer| {
        write_enforcement(*value, writer)
    })?;
    writer.optional(value.lease_size.as_ref(), |value, writer| {
        writer.u64(*value)
    })
}

fn write_budget(
    value: &BudgetData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.u64(value.limit)?;
    writer.u64(value.window_ms)?;
    writer.optional(value.enforcement.as_ref(), |value, writer| {
        write_enforcement(*value, writer)
    })?;
    writer.optional(value.lease_size.as_ref(), |value, writer| {
        writer.u64(*value)
    })?;
    writer.optional(value.unit.as_ref(), |value, writer| {
        writer.raw(value, 65535)
    })
}

fn write_circuit(
    value: &CircuitData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.u64(value.failure_threshold)?;
    writer.u64(value.open_ms)?;
    writer.optional(value.window_ms.as_ref(), |value, writer| writer.u64(*value))?;
    writer.optional(value.min_calls.as_ref(), |value, writer| writer.u64(*value))?;
    writer.optional(value.failure_rate_pct.as_ref(), |value, writer| {
        writer.byte(*value)
    })?;
    writer.optional(value.latency_threshold_ms.as_ref(), |value, writer| {
        writer.u64(*value)
    })?;
    writer.optional(value.half_open_max_probes.as_ref(), |value, writer| {
        writer.u64(*value)
    })?;
    writer.optional(
        value.half_open_success_threshold.as_ref(),
        |value, writer| writer.u64(*value),
    )?;
    writer.optional(value.error_classes.as_ref(), |names, writer| {
        write_names(names, 1000, 256, writer)
    })
}

fn write_approvals(
    value: &ApprovalsData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.optional(value.effects.as_ref(), |names, writer| {
        write_names(names, 1000, 65535, writer)
    })?;
    writer.optional(value.states.as_ref(), |names, writer| {
        write_names(names, 1000, 65535, writer)
    })?;
    writer.optional(value.assignees.as_ref(), |names, writer| {
        write_names(names, 1000, 65535, writer)
    })?;
    writer.optional(value.timeout_ms.as_ref(), |value, writer| {
        writer.u64(*value)
    })
}

fn write_enforcement(
    value: LimitEnforcement,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.byte(match value {
        LimitEnforcement::StrictGlobal => 0,
        LimitEnforcement::ApproximateGlobal => 1,
    })
}
