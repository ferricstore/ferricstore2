//! Validated immutable workflow definitions and explicit policy patch values.
//!
//! This M1 view covers modes/retry/retention/version/max-active. Governance and
//! indexed attribute/state metadata have separate required M5 implementations;
//! an outer schema must reject unsupported supplied fields, never drop them.

use std::collections::BTreeMap;

use crate::retention::{ResolvedRetention, RetentionOverride};
use crate::value::DomainError;

use super::installation::{InstallCommand, InstallMode, PolicyError, PolicyLimits};
use super::{BackoffKind, RetryOverride, RetryPolicy};

/// Explicit patch presence, including source null clearing of optional fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum FieldPatch<T> {
    /// Keep old value for patch; use default/absence for replacement.
    #[default]
    Unchanged,
    /// Replace or recursively merge a supplied value.
    Set(T),
    /// Remove an optional value, preserving null versus omission.
    Clear,
}

/// Source state execution mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateMode {
    /// Every due eligible record may execute independently.
    Parallel,
    /// Only the authoritative first lane member may execute.
    Fifo,
}

/// Customer policy version, separate from internal generation and encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyVersion {
    /// Nonempty opaque binary customer label.
    Bytes(Vec<u8>),
    /// Nonnegative customer version in the Rust numeric representation.
    Number(u64),
}

/// Partial backoff map, deeply merged before source default normalization.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackoffPatch {
    /// Optional growth kind.
    pub kind: Option<BackoffKind>,
    /// Optional base delay, including zero.
    pub base_ms: Option<u64>,
    /// Optional maximum delay, including zero.
    pub max_ms: Option<u64>,
    /// Optional jitter percent, including zero.
    pub jitter_pct: Option<u8>,
}

/// Partial retry map; omission differs from a supplied empty backoff map.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RetryPatch {
    /// Optional inclusive retry count.
    pub max_retries: Option<u16>,
    /// Optional recursively merged backoff; an empty map normalizes defaults.
    pub backoff: Option<BackoffPatch>,
    /// Optional nonempty exhaustion target, except `running`.
    pub exhausted_to: Option<Vec<u8>>,
}

/// State-level patch; type-only max-active cannot be supplied here.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatePolicyPatch {
    /// Optional state mode; present clearing is rejected, while omission inherits.
    pub mode: FieldPatch<StateMode>,
    /// Recursive retry patch or complete clearing.
    pub retry: FieldPatch<RetryPatch>,
    /// Recursive public retention patch or complete clearing.
    pub retention: FieldPatch<RetentionOverride>,
}

/// Source null state deletion is separate from an empty state policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateChange {
    /// Merge supplied state fields into an existing state policy.
    Patch(StatePolicyPatch),
    /// Delete this policy state during patch; replacement rejects explicit null.
    Remove,
}

/// Normalized supported policy patch with byte-sorted unique state names.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PolicyPatch {
    /// Optional customer version patch.
    pub version: FieldPatch<PolicyVersion>,
    /// Optional positive type-only active duration; clear means infinity.
    pub max_active_ms: FieldPatch<u64>,
    /// Recursive type retry patch or clearing.
    pub retry: FieldPatch<RetryPatch>,
    /// Recursive type retention patch or clearing.
    pub retention: FieldPatch<RetentionOverride>,
    /// Deterministically ordered unique state patch/deletion entries.
    pub states: BTreeMap<Vec<u8>, StateChange>,
}

/// Normalized immutable state-policy read value within a validated snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatePolicy {
    /// Absent resolves to parallel.
    pub mode: Option<StateMode>,
    /// Normalized type/state override fields.
    pub retry: RetryOverride,
    /// Public retention fields; internal hot-history field is omitted.
    pub retention: RetentionOverride,
}

/// Immutable supported policy snapshot; construction/patching validates it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicySnapshot {
    pub(super) workflow_type: Vec<u8>,
    pub(super) version: Option<PolicyVersion>,
    pub(super) max_active_ms: Option<u64>,
    pub(super) retry: RetryOverride,
    pub(super) retention: RetentionOverride,
    pub(super) states: BTreeMap<Vec<u8>, StatePolicy>,
}

impl PolicySnapshot {
    pub(super) fn empty(workflow_type: Vec<u8>) -> Self {
        Self {
            workflow_type,
            version: None,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states: BTreeMap::new(),
        }
    }
    /// Borrows the fixed opaque workflow type.
    #[must_use]
    pub fn workflow_type(&self) -> &[u8] {
        &self.workflow_type
    }
    /// Borrows optional customer version, independently of install generation.
    #[must_use]
    pub const fn version(&self) -> Option<&PolicyVersion> {
        self.version.as_ref()
    }
    /// Reads optional type-only active duration; absence means infinity.
    #[must_use]
    pub const fn max_active_ms(&self) -> Option<u64> {
        self.max_active_ms
    }
    /// Borrows normalized type retry fields without resolving or cloning them.
    #[must_use]
    pub const fn type_retry(&self) -> &RetryOverride {
        &self.retry
    }
    /// Reads normalized type retention overrides without applying defaults.
    #[must_use]
    pub const fn type_retention(&self) -> RetentionOverride {
        self.retention
    }
    /// Borrows unique normalized states in stable binary order.
    #[must_use]
    pub const fn states(&self) -> &BTreeMap<Vec<u8>, StatePolicy> {
        &self.states
    }
    /// Resolves execution mode without inspecting any current record or clock.
    #[must_use]
    pub fn state_mode(&self, state: &[u8]) -> StateMode {
        self.states
            .get(state)
            .and_then(|policy| policy.mode)
            .unwrap_or(StateMode::Parallel)
    }
    /// Resolves source default/type/state/command retry in source order.
    ///
    /// # Errors
    /// Rejects malformed typed command override before resolving.
    pub fn resolve_retry(
        &self,
        state: Option<&[u8]>,
        command: &RetryOverride,
    ) -> Result<RetryPolicy, DomainError> {
        command.validate()?;
        let mut policy = RetryPolicy::default();
        super::merge(&mut policy, &self.retry);
        if let Some(state) = state.and_then(|state| self.states.get(state)) {
            super::merge(&mut policy, &state.retry);
        }
        super::merge(&mut policy, command);
        Ok(policy)
    }
    /// Resolves default/type/state public retention without internal hot caps.
    #[must_use]
    pub fn resolve_retention(&self, state: Option<&[u8]>) -> ResolvedRetention {
        let mut policy = ResolvedRetention::default();
        crate::retention::merge(&mut policy, self.retention);
        if let Some(state) = state.and_then(|state| self.states.get(state)) {
            crate::retention::merge(&mut policy, state.retention);
        }
        policy
    }
}

/// Validated logical states and explicit initial state, independent of records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowDefinition {
    workflow_type: Vec<u8>,
    initial_state: Vec<u8>,
    states: BTreeMap<Vec<u8>, StatePolicyPatch>,
}

impl WorkflowDefinition {
    /// Validates opaque type, declared initial state and supported state settings.
    ///
    /// # Errors
    /// Rejects absent/empty/running initial state or invalid/budgeted state fields.
    pub fn new(
        workflow_type: Vec<u8>,
        initial_state: Vec<u8>,
        states: BTreeMap<Vec<u8>, StatePolicyPatch>,
        limits: PolicyLimits,
    ) -> Result<Self, PolicyError> {
        if initial_state.is_empty()
            || initial_state == b"running"
            || !states.contains_key(&initial_state)
        {
            return Err(PolicyError::InvalidInitialState);
        }
        super::install_validation::validate_definition(&workflow_type, &states, limits)?;
        Ok(Self {
            workflow_type,
            initial_state,
            states,
        })
    }
    /// Borrows explicit initial logical handler state.
    #[must_use]
    pub fn initial_state(&self) -> &[u8] {
        &self.initial_state
    }
    /// Builds the source DSL's default replace install; callers may choose patch.
    #[must_use]
    pub fn install_command(&self) -> InstallCommand {
        InstallCommand {
            workflow_type: self.workflow_type.clone(),
            expected_generation: None,
            mode: InstallMode::Replace,
            patch: PolicyPatch {
                states: self
                    .states
                    .iter()
                    .map(|(state, policy)| (state.clone(), StateChange::Patch(policy.clone())))
                    .collect(),
                ..PolicyPatch::default()
            },
        }
    }
}
