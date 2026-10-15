//! Typed normalized-content refusals and separate current-value diagnostics.

use std::fmt;

use crate::policy::installation::PolicyError;

/// Which normalized rule map failed admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceRuleKind {
    /// Limit rules.
    Limits,
    /// Budget rules.
    Budgets,
    /// Circuit rules.
    Circuits,
}

/// Exact known field associated with normalized-content rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceField {
    /// Allowed effect names.
    Allowed,
    /// Denied effect names.
    Denied,
    /// Approval effect names.
    ApprovalEffects,
    /// Approval state names.
    ApprovalStates,
    /// Approval assignees.
    Assignees,
    /// Circuit error classes.
    ErrorClasses,
    /// Nonnegative limit quantity.
    Limit,
    /// Positive credit quantity.
    LeaseSize,
    /// Positive observation window.
    WindowMillis,
    /// Opaque budget unit.
    Unit,
    /// Positive circuit failure threshold.
    FailureThreshold,
    /// Positive circuit open duration.
    OpenMillis,
    /// Positive minimum call count.
    MinCalls,
    /// Failure percentage.
    FailureRate,
    /// Positive latency threshold.
    LatencyMillis,
    /// Positive maximum half-open probe count.
    HalfOpenProbes,
    /// Positive half-open success threshold.
    HalfOpenSuccesses,
    /// Positive approval timeout.
    ApprovalTimeout,
}

/// Ordered circuit tracking consistency refusals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CircuitTrackingFailure {
    /// Minimum calls exceed the source tracking capacity.
    MinCallsAbove64,
    /// A large threshold needs failure-rate tracking.
    ThresholdAbove64WithoutRate,
    /// A large rate-tracked threshold needs explicit minimum calls.
    MissingMinCallsForLargeThreshold,
}

/// Normalized input errors and operational current-profile/work diagnoses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicySchemaError {
    /// Existing core policy field rejection.
    Policy(PolicyError),
    /// Current content or a bounded query is incoherent under this profile.
    InvalidAuthority,
    /// One rule map exceeds 1000 entries.
    RuleCount(GovernanceRuleKind),
    /// One normalized list exceeds 1000 entries.
    ListCount(GovernanceField),
    /// A rule name is empty.
    InvalidName(GovernanceRuleKind),
    /// A rule name exceeds 65535 bytes.
    NameBytes(GovernanceRuleKind),
    /// A required opaque list or unit value is empty.
    InvalidListValue(GovernanceField),
    /// An opaque list or unit value exceeds its field's byte cap.
    ListBytes(GovernanceField),
    /// Supplied normalized list values are not unique.
    NonCanonicalList(GovernanceField),
    /// An integer violates its field's required bounds.
    InvalidInteger(GovernanceField),
    /// A supplied failure percentage is outside one through 100.
    InvalidPercent,
    /// Circuit tracking fields are inconsistent.
    CircuitTracking(CircuitTrackingFailure),
    /// Indexed attributes violate normalized count/name admission.
    InvalidIndexedAttributes,
    /// Indexed state metadata violates normalized name admission.
    InvalidIndexedStateMeta,
    /// One governance group's actual raw leaf sum exceeds 1 MiB.
    GovernanceBytes,
    /// Actual snapshot bytes or the complete raw leaf sum exceed 256 KiB.
    SnapshotBytes,
    /// Actual complete encoded envelope exceeds 263168 bytes.
    EnvelopeBytes,
    /// The supplied snapshot range is reversed or outside the actual owner.
    InvalidRange,
    /// Checked logical length or work arithmetic overflows.
    LengthOverflow,
    /// The operation and account use different immutable profiles.
    ProfileMismatch,
    /// A prospective logical work admission exceeds the immutable profile.
    WorkBytes {
        /// Prospective cumulative work plus peak scratch.
        bytes: u64,
        /// Immutable maximum for this account.
        maximum: u64,
    },
}

impl fmt::Display for PolicySchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = match self {
            Self::ListCount(field)
            | Self::InvalidListValue(field)
            | Self::ListBytes(field)
            | Self::NonCanonicalList(field)
            | Self::InvalidInteger(field) => match field {
                GovernanceField::Allowed => "allowed",
                GovernanceField::Denied => "denied",
                GovernanceField::ApprovalEffects => "approval effects",
                GovernanceField::ApprovalStates => "approval states",
                GovernanceField::Assignees => "assignees",
                GovernanceField::ErrorClasses => "error_classes",
                GovernanceField::Limit => "limit",
                GovernanceField::LeaseSize => "lease_size",
                GovernanceField::WindowMillis => "window_ms",
                GovernanceField::Unit => "unit",
                GovernanceField::FailureThreshold => "failure_threshold",
                GovernanceField::OpenMillis => "open_ms",
                GovernanceField::MinCalls => "min_calls",
                GovernanceField::FailureRate => "failure_rate_pct",
                GovernanceField::LatencyMillis => "latency_threshold_ms",
                GovernanceField::HalfOpenProbes => "half_open_max_probes",
                GovernanceField::HalfOpenSuccesses => "half_open_success_threshold",
                GovernanceField::ApprovalTimeout => "approval timeout_ms",
            },
            _ => "",
        };
        match self {
            Self::Policy(error) => fmt::Display::fmt(error, formatter),
            Self::WorkBytes { bytes, maximum } => write!(
                formatter,
                "complete policy work budget exceeded ({bytes}, maximum {maximum})"
            ),
            Self::RuleCount(kind) => write!(
                formatter,
                "ERR flow governance {kind:?} rule count exceeds 1000"
            ),
            Self::InvalidName(kind) => write!(
                formatter,
                "ERR flow governance {kind:?} rule name must be nonempty"
            ),
            Self::NameBytes(kind) => write!(
                formatter,
                "ERR flow governance {kind:?} rule name exceeds 65535 bytes"
            ),
            Self::ListCount(_) => write!(
                formatter,
                "ERR flow governance {field} list count exceeds 1000"
            ),
            Self::InvalidListValue(_) => write!(
                formatter,
                "ERR flow governance {field} must contain nonempty names"
            ),
            Self::ListBytes(_) => write!(
                formatter,
                "ERR flow governance {field} name exceeds its byte maximum"
            ),
            Self::NonCanonicalList(_) => write!(
                formatter,
                "ERR normalized flow governance {field} names must be unique"
            ),
            Self::InvalidInteger(_) => write!(
                formatter,
                "ERR flow governance {field} is outside its integer bounds"
            ),
            Self::CircuitTracking(failure) => formatter.write_str(match failure {
                CircuitTrackingFailure::MinCallsAbove64 => "ERR circuit min_calls exceeds 64",
                CircuitTrackingFailure::ThresholdAbove64WithoutRate => {
                    "ERR circuit failure_threshold above 64 requires failure_rate_pct"
                }
                CircuitTrackingFailure::MissingMinCallsForLargeThreshold => {
                    "ERR circuit failure_threshold above 64 requires min_calls"
                }
            }),
            error => formatter.write_str(match error {
                Self::InvalidAuthority => "invalid complete policy authority",
                Self::InvalidPercent => "ERR circuit failure_rate_pct must be between 1 and 100",
                Self::InvalidIndexedAttributes => "ERR invalid normalized indexed attributes",
                Self::InvalidIndexedStateMeta => "ERR invalid normalized indexed state metadata",
                Self::GovernanceBytes => "ERR normalized governance raw bytes exceed 1048576",
                Self::SnapshotBytes => "ERR complete policy snapshot bytes exceed 262144",
                Self::EnvelopeBytes => "ERR complete policy envelope bytes exceed 263168",
                Self::InvalidRange => "invalid complete policy snapshot range",
                Self::LengthOverflow => "complete policy byte length overflow",
                Self::ProfileMismatch => "complete policy account profile mismatch",
                _ => "complete policy field rejection",
            }),
        }
    }
}

impl std::error::Error for PolicySchemaError {}
