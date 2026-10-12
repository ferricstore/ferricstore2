//! Presence-preserving normalized governance groups and state content.

use std::collections::BTreeMap;

use crate::policy::RetryOverride;
use crate::policy::definition::StateMode;
use crate::retention::RetentionOverride;
use crate::workflow::active::LimitEnforcement;

/// Normalized content attached to one unique logical-state key.
#[derive(Eq, PartialEq)]
pub struct CompleteStatePolicyData {
    /// Optional execution mode; absence resolves to parallel.
    pub mode: Option<StateMode>,
    /// Optional normalized retry group, preserving a supplied empty group.
    pub retry: Option<RetryOverride>,
    /// Optional normalized retention group, preserving a supplied empty group.
    pub retention: Option<RetentionOverride>,
    /// Optional governance belonging to this exact state key.
    pub governance: Option<GovernanceData>,
}

/// Requested governance mode, without service-execution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceMode {
    /// Apply the source minimal defaults.
    Minimal,
    /// Request ledger behavior.
    Ledger,
    /// Request the complete governance behavior.
    Full,
}

/// Requested denial-audit behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DenialAuditMode {
    /// Do not request denial events.
    Off,
    /// Request sampled denial events.
    Sampled,
    /// Request every denial event.
    All,
}

/// All normalized known governance groups, preserving optional presence.
#[derive(Eq, PartialEq)]
pub struct GovernanceData {
    /// Optional mode override.
    pub mode: Option<GovernanceMode>,
    /// Optional effect names and idempotency requirement.
    pub effects: Option<EffectsData>,
    /// Optional audit scalars.
    pub audit: Option<AuditData>,
    /// Optional unique opaque limit-rule names.
    pub limits: Option<BTreeMap<Vec<u8>, LimitData>>,
    /// Optional unique opaque budget-rule names.
    pub budgets: Option<BTreeMap<Vec<u8>, BudgetData>>,
    /// Optional unique opaque circuit-rule names.
    pub circuits: Option<BTreeMap<Vec<u8>, CircuitData>>,
    /// Optional approval names and timeout.
    pub approvals: Option<ApprovalsData>,
}

/// Normalized known effect group; an empty supplied list overrides inheritance.
#[derive(Eq, PartialEq)]
pub struct EffectsData {
    /// Optional unique allowed names in first-occurrence order.
    pub allowed: Option<Vec<Vec<u8>>>,
    /// Optional unique denied names in first-occurrence order.
    pub denied: Option<Vec<Vec<u8>>>,
    /// Optional requirement, preserving an explicit false.
    pub require_idempotency_key: Option<bool>,
}

/// Normalized audit scalar overrides.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AuditData {
    /// Optional request for effect events, preserving false.
    pub effect_events: Option<bool>,
    /// Optional denial mode.
    pub denials: Option<DenialAuditMode>,
}

/// Normalized limit rule; zero is a represented limit, not omission.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct LimitData {
    /// Inclusive nonnegative exact source quantity.
    pub limit: u64,
    /// Optional enforcement override.
    pub enforcement: Option<LimitEnforcement>,
    /// Optional positive credit quantity, separate from caller lease duration.
    pub lease_size: Option<u64>,
}

/// Normalized budget rule with an original optional opaque unit owner.
#[derive(Eq, PartialEq)]
pub struct BudgetData {
    /// Nonnegative exact source quantity, including zero.
    pub limit: u64,
    /// Positive exact source window in milliseconds.
    pub window_ms: u64,
    /// Optional enforcement override.
    pub enforcement: Option<LimitEnforcement>,
    /// Optional positive credit quantity.
    pub lease_size: Option<u64>,
    /// Optional nonempty opaque unit name.
    pub unit: Option<Vec<u8>>,
}

/// Normalized circuit rule; optional classes preserve supplied empty lists.
#[derive(Eq, PartialEq)]
pub struct CircuitData {
    /// Positive failure threshold.
    pub failure_threshold: u64,
    /// Positive open duration in milliseconds.
    pub open_ms: u64,
    /// Optional positive observation window.
    pub window_ms: Option<u64>,
    /// Optional positive minimum call count, at most 64.
    pub min_calls: Option<u64>,
    /// Optional percentage from one through 100.
    pub failure_rate_pct: Option<u8>,
    /// Optional positive latency threshold.
    pub latency_threshold_ms: Option<u64>,
    /// Optional positive maximum half-open probe count.
    pub half_open_max_probes: Option<u64>,
    /// Optional positive half-open success threshold.
    pub half_open_success_threshold: Option<u64>,
    /// Optional unique opaque classes in original normalized order.
    pub error_classes: Option<Vec<Vec<u8>>>,
}

/// Normalized approval group; this value performs no approval service work.
#[derive(Eq, PartialEq)]
pub struct ApprovalsData {
    /// Optional unique effects requiring approval.
    pub effects: Option<Vec<Vec<u8>>>,
    /// Optional unique associated states.
    pub states: Option<Vec<Vec<u8>>>,
    /// Optional unique assignee names.
    pub assignees: Option<Vec<Vec<u8>>>,
    /// Optional positive timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}
