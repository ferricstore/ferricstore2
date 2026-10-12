//! Scalar rule merges borrowing units and class lists from original owners.

use crate::workflow::active::LimitEnforcement;

use super::{BudgetData, CircuitData, LimitData};

/// Resolved budget content without executing or authorizing a budget spend.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedBudget<'v> {
    limit: u64,
    window_ms: u64,
    enforcement: Option<LimitEnforcement>,
    lease_size: Option<u64>,
    unit: Option<&'v [u8]>,
}

impl<'v> ResolvedBudget<'v> {
    /// Reads the resolved quantity, including zero.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }
    /// Reads the positive observation window.
    #[must_use]
    pub const fn window_ms(self) -> u64 {
        self.window_ms
    }
    /// Reads optional enforcement without establishing external authority.
    #[must_use]
    pub const fn enforcement(self) -> Option<LimitEnforcement> {
        self.enforcement
    }
    /// Reads optional positive credit quantity, not caller TTL.
    #[must_use]
    pub const fn lease_size(self) -> Option<u64> {
        self.lease_size
    }
    /// Borrows the original selected unit independently of this descriptor.
    #[must_use]
    pub const fn unit(self) -> Option<&'v [u8]> {
        self.unit
    }
}

/// Resolved circuit content; no circuit service or effect is executed.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedCircuit<'v> {
    failure_threshold: u64,
    open_ms: u64,
    window_ms: Option<u64>,
    min_calls: Option<u64>,
    failure_rate_pct: Option<u8>,
    latency_threshold_ms: Option<u64>,
    half_open_max_probes: Option<u64>,
    half_open_success_threshold: Option<u64>,
    error_classes: Option<&'v [Vec<u8>]>,
}

impl<'v> ResolvedCircuit<'v> {
    /// Reads the positive failure threshold.
    #[must_use]
    pub const fn failure_threshold(self) -> u64 {
        self.failure_threshold
    }
    /// Reads the positive open duration.
    #[must_use]
    pub const fn open_ms(self) -> u64 {
        self.open_ms
    }
    /// Reads the optional positive observation window.
    #[must_use]
    pub const fn window_ms(self) -> Option<u64> {
        self.window_ms
    }
    /// Reads the optional minimum call count.
    #[must_use]
    pub const fn min_calls(self) -> Option<u64> {
        self.min_calls
    }
    /// Reads the optional one-through-100 failure percentage.
    #[must_use]
    pub const fn failure_rate_pct(self) -> Option<u8> {
        self.failure_rate_pct
    }
    /// Reads the optional positive latency threshold.
    #[must_use]
    pub const fn latency_threshold_ms(self) -> Option<u64> {
        self.latency_threshold_ms
    }
    /// Reads the optional positive half-open probe maximum.
    #[must_use]
    pub const fn half_open_max_probes(self) -> Option<u64> {
        self.half_open_max_probes
    }
    /// Reads the optional positive half-open success threshold.
    #[must_use]
    pub const fn half_open_success_threshold(self) -> Option<u64> {
        self.half_open_success_threshold
    }
    /// Borrows original classes; a present empty list remains present.
    #[must_use]
    pub const fn error_classes(self) -> Option<&'v [Vec<u8>]> {
        self.error_classes
    }
}

pub(super) fn merge_limit(
    type_rule: Option<&LimitData>,
    state_rule: Option<&LimitData>,
) -> Option<LimitData> {
    let selected = state_rule.or(type_rule)?;
    Some(LimitData {
        limit: selected.limit,
        enforcement: state_rule
            .and_then(|rule| rule.enforcement)
            .or_else(|| type_rule.and_then(|rule| rule.enforcement)),
        lease_size: state_rule
            .and_then(|rule| rule.lease_size)
            .or_else(|| type_rule.and_then(|rule| rule.lease_size)),
    })
}

pub(super) fn merge_budget<'v>(
    type_rule: Option<&'v BudgetData>,
    state_rule: Option<&'v BudgetData>,
) -> Option<ResolvedBudget<'v>> {
    let selected = state_rule.or(type_rule)?;
    Some(ResolvedBudget {
        limit: selected.limit,
        window_ms: selected.window_ms,
        enforcement: state_rule
            .and_then(|rule| rule.enforcement)
            .or_else(|| type_rule.and_then(|rule| rule.enforcement)),
        lease_size: state_rule
            .and_then(|rule| rule.lease_size)
            .or_else(|| type_rule.and_then(|rule| rule.lease_size)),
        unit: state_rule
            .and_then(|rule| rule.unit.as_deref())
            .or_else(|| type_rule.and_then(|rule| rule.unit.as_deref())),
    })
}

pub(super) fn merge_circuit<'v>(
    type_rule: Option<&'v CircuitData>,
    state_rule: Option<&'v CircuitData>,
) -> Option<ResolvedCircuit<'v>> {
    let selected = state_rule.or(type_rule)?;
    Some(ResolvedCircuit {
        failure_threshold: selected.failure_threshold,
        open_ms: selected.open_ms,
        window_ms: state_rule
            .and_then(|rule| rule.window_ms)
            .or_else(|| type_rule.and_then(|rule| rule.window_ms)),
        min_calls: state_rule
            .and_then(|rule| rule.min_calls)
            .or_else(|| type_rule.and_then(|rule| rule.min_calls)),
        failure_rate_pct: state_rule
            .and_then(|rule| rule.failure_rate_pct)
            .or_else(|| type_rule.and_then(|rule| rule.failure_rate_pct)),
        latency_threshold_ms: state_rule
            .and_then(|rule| rule.latency_threshold_ms)
            .or_else(|| type_rule.and_then(|rule| rule.latency_threshold_ms)),
        half_open_max_probes: state_rule
            .and_then(|rule| rule.half_open_max_probes)
            .or_else(|| type_rule.and_then(|rule| rule.half_open_max_probes)),
        half_open_success_threshold: state_rule
            .and_then(|rule| rule.half_open_success_threshold)
            .or_else(|| type_rule.and_then(|rule| rule.half_open_success_threshold)),
        error_classes: state_rule
            .and_then(|rule| rule.error_classes.as_deref())
            .or_else(|| type_rule.and_then(|rule| rule.error_classes.as_deref())),
    })
}
