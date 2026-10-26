//! Creation-time lifecycle resolution over borrowed normalized policy layers.

use std::fmt;

use crate::policy::definition::PolicySnapshot;
use crate::retention::{
    MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS, ResolvedRetention, RetentionOverride,
};
use crate::value::DomainError;
use crate::workflow::admission::AdmissionError;

/// Caller max-active presence; infinity differs from inheritance.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum MaxActiveOverride {
    /// Resolve the installed type policy.
    Inherit,
    /// Disable the captured deadline.
    Infinite,
    /// Capture this positive bounded duration.
    Millis(u64),
}

/// Trusted configured defaults/caps, separate from installed public policy.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct LifecycleDefaults {
    /// Default terminal retention duration.
    pub ttl_ms: u64,
    /// Normalized default hot count; zero is valid.
    pub history_hot_max_events: u32,
    /// Default complete history count.
    pub history_max_events: u32,
    /// Raw configured hot maximum; 10,000 is a default rather than a hard bound.
    pub max_history_hot_max_events: u64,
    /// Configured complete-history maximum, at most the source hard ceiling.
    pub max_history_max_events: u32,
}

impl LifecycleDefaults {
    /// Checks recorded configuration without policy access or substitution.
    ///
    /// # Errors
    /// Refuses invalid configured retention defaults or history caps.
    pub fn validate(self) -> Result<(), ActiveValueError> {
        validate_defaults(self)
    }
}

/// Explicit command overrides, including the internal hot-history command field.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct LifecycleOverrides {
    /// Command max-active precedence.
    pub active: MaxActiveOverride,
    /// Public terminal retention overrides.
    pub retention: RetentionOverride,
    /// Internal supplied hot count, preserved before source checks and clamping.
    pub history_hot_max_events: Option<u64>,
}

/// Immutable creation-time values; later policy changes never replace them.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct CapturedLifecycle {
    max_active_ms: Option<u64>,
    retention_ttl_ms: u64,
    history_hot_max_events: u32,
    history_max_events: u32,
}
impl CapturedLifecycle {
    /// Captured duration; absence means infinity.
    #[must_use]
    pub const fn max_active_ms(self) -> Option<u64> {
        self.max_active_ms
    }
    /// Captured terminal retention duration.
    #[must_use]
    pub const fn retention_ttl_ms(self) -> u64 {
        self.retention_ttl_ms
    }
    /// Captured hot-history cap, including zero.
    #[must_use]
    pub const fn history_hot_max_events(self) -> u32 {
        self.history_hot_max_events
    }
    /// Captured complete-history positive cap.
    #[must_use]
    pub const fn history_max_events(self) -> u32 {
        self.history_max_events
    }
}

/// Pure value/capture error; it does not certify consumed apply authority.
#[derive(Debug, Eq, PartialEq)]
pub enum ActiveValueError {
    /// Existing core value rejection.
    Domain(DomainError),
    /// Selected actual-byte admission failure.
    Admission(AdmissionError),
    /// Borrowed policy belongs to another type.
    WrongPolicyType,
    /// Invalid captured active duration.
    InvalidCapturedActive,
    /// Invalid configured/resolved retention.
    InvalidCapturedRetention,
    /// Invalid lineage group or summary.
    InvalidLineage,
    /// Invalid local reservation fields.
    InvalidReservation,
    /// Invalid represented value reference.
    InvalidRefs,
    /// Invalid restored core/witness/carry consistency; no schema proof is inferred.
    InvalidExtensions,
}
impl fmt::Display for ActiveValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => e.fmt(f),
            Self::Admission(e) => e.fmt(f),
            _ => f.write_str(match self {
                Self::WrongPolicyType => "captured policy belongs to another workflow type",
                Self::InvalidCapturedActive => "invalid captured max-active duration",
                Self::InvalidCapturedRetention => "invalid captured retention",
                Self::InvalidLineage => "invalid captured lineage",
                Self::InvalidReservation => "invalid governance reservation",
                Self::InvalidRefs => "invalid active value reference",
                Self::InvalidExtensions => "invalid restored value consistency",
                Self::Domain(_) | Self::Admission(_) => "invalid active value",
            }),
        }
    }
}
impl std::error::Error for ActiveValueError {}

/// Restores exact birth scalars using their recorded creation configuration.
///
/// This validates ordinary data; storage separately establishes its binding.
///
/// # Errors
/// Refuses invalid configuration, active duration or captured retention counts.
pub fn restore_captured_lifecycle(
    max_active_ms: Option<u64>,
    retention_ttl_ms: u64,
    history_hot_max_events: u32,
    history_max_events: u32,
    defaults: LifecycleDefaults,
) -> Result<CapturedLifecycle, ActiveValueError> {
    validate_defaults(defaults)?;
    let max_active_ms = resolve_active(max_active_ms, MaxActiveOverride::Inherit)?;
    RetentionOverride {
        ttl_ms: Some(retention_ttl_ms),
        history_max_events: Some(history_max_events),
    }
    .validate(defaults.max_history_max_events)
    .map_err(|_| ActiveValueError::InvalidCapturedRetention)?;
    if u64::from(history_hot_max_events) > defaults.max_history_hot_max_events
        || history_hot_max_events > history_max_events
    {
        return Err(ActiveValueError::InvalidCapturedRetention);
    }
    Ok(CapturedLifecycle {
        max_active_ms,
        retention_ttl_ms,
        history_hot_max_events,
        history_max_events,
    })
}

/// Resolves defaults/type/state/command before retaining any new byte owner.
///
/// # Errors
/// Rejects foreign type or invalid supplied/configured duration/history values.
pub fn capture_lifecycle(
    policy: Option<&PolicySnapshot>,
    workflow_type: &[u8],
    initial_state: &[u8],
    overrides: LifecycleOverrides,
    defaults: LifecycleDefaults,
) -> Result<CapturedLifecycle, ActiveValueError> {
    capture_lifecycle_layers(
        policy.map(PolicySnapshot::workflow_type),
        workflow_type,
        policy.and_then(PolicySnapshot::max_active_ms),
        policy.map_or_else(RetentionOverride::default, PolicySnapshot::type_retention),
        policy.and_then(|p| p.states().get(initial_state).map(|s| s.retention)),
        overrides,
        defaults,
    )
}

/// Resolves already selected scalar policy layers without owning a snapshot.
///
/// Policy admission and prospective work remain the caller's responsibility.
pub(crate) fn capture_lifecycle_layers(
    policy_type: Option<&[u8]>,
    workflow_type: &[u8],
    max_active_ms: Option<u64>,
    type_retention: RetentionOverride,
    state_retention: Option<RetentionOverride>,
    overrides: LifecycleOverrides,
    defaults: LifecycleDefaults,
) -> Result<CapturedLifecycle, ActiveValueError> {
    validate_defaults(defaults)?;
    if policy_type.is_some_and(|kind| kind != workflow_type) {
        return Err(ActiveValueError::WrongPolicyType);
    }
    let max_active_ms = resolve_active(max_active_ms, overrides.active)?;
    let (retention, hot) =
        resolve_captured_retention(type_retention, state_retention, overrides, defaults)?;
    Ok(CapturedLifecycle {
        max_active_ms,
        retention_ttl_ms: retention.ttl_ms,
        history_hot_max_events: hot,
        history_max_events: retention.history_max_events,
    })
}

fn validate_defaults(defaults: LifecycleDefaults) -> Result<(), ActiveValueError> {
    if defaults.max_history_hot_max_events == 0
        || defaults.max_history_max_events == 0
        || defaults.max_history_max_events > MAX_HISTORY_EVENTS
        || defaults.history_max_events > defaults.max_history_max_events
        || u64::from(defaults.history_hot_max_events) > defaults.max_history_hot_max_events
        || defaults.history_hot_max_events > defaults.history_max_events
    {
        return Err(ActiveValueError::InvalidCapturedRetention);
    }
    RetentionOverride {
        ttl_ms: Some(defaults.ttl_ms),
        history_max_events: Some(defaults.history_max_events),
    }
    .validate(defaults.max_history_max_events)
    .map_err(|_| ActiveValueError::InvalidCapturedRetention)
}

fn resolve_active(
    max_active_ms: Option<u64>,
    command: MaxActiveOverride,
) -> Result<Option<u64>, ActiveValueError> {
    let value = match command {
        MaxActiveOverride::Inherit => max_active_ms,
        MaxActiveOverride::Infinite => None,
        MaxActiveOverride::Millis(n) => Some(n),
    };
    if value.is_some_and(|n| n == 0 || n > MAX_RETENTION_MILLIS) {
        Err(ActiveValueError::InvalidCapturedActive)
    } else {
        Ok(value)
    }
}

fn resolve_captured_retention(
    type_retention: RetentionOverride,
    state_retention: Option<RetentionOverride>,
    command: LifecycleOverrides,
    defaults: LifecycleDefaults,
) -> Result<(ResolvedRetention, u32), ActiveValueError> {
    command
        .retention
        .validate(MAX_HISTORY_EVENTS)
        .map_err(|_| ActiveValueError::InvalidCapturedRetention)?;
    if command
        .history_hot_max_events
        .zip(command.retention.history_max_events)
        .is_some_and(|(hot, total)| hot > u64::from(total))
    {
        return Err(ActiveValueError::InvalidCapturedRetention);
    }
    let mut result = ResolvedRetention {
        ttl_ms: defaults.ttl_ms,
        history_max_events: defaults.history_max_events,
    };
    crate::retention::merge(&mut result, type_retention);
    if let Some(state) = state_retention {
        crate::retention::merge(&mut result, state);
    }
    crate::retention::merge(&mut result, command.retention);
    result.history_max_events = result
        .history_max_events
        .min(defaults.max_history_max_events);
    let hot = command
        .history_hot_max_events
        .unwrap_or(u64::from(defaults.history_hot_max_events))
        .min(defaults.max_history_hot_max_events)
        .min(u64::from(result.history_max_events));
    Ok((
        result,
        u32::try_from(hot).map_err(|_| ActiveValueError::InvalidCapturedRetention)?,
    ))
}
