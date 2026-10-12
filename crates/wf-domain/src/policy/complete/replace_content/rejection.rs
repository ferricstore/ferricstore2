//! Immutable reached content reasons and one freshly paid response buffer.

use crate::policy::installation::PolicyError;
use crate::retention::RetentionError;
use crate::value::{DomainError, InputField, RetryPolicyField};

use super::super::{
    CircuitTrackingFailure, CompletePolicyEncodingError, GovernanceField, GovernanceRuleKind,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use super::charge_schema;

/// Mathematical source-content reason, separate from operational origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletePolicyReplaceRejectionKind {
    /// Required workflow type is empty.
    EmptyType,
    /// Supplied customer version bytes are empty.
    InvalidVersion,
    /// Active duration is outside the source range.
    InvalidMaxActive,
    /// Retry count exceeds 1000.
    RetryMaxRetries,
    /// Base backoff exceeds its source maximum.
    RetryBaseMillis,
    /// Maximum backoff exceeds its source maximum.
    RetryMaxMillis,
    /// Jitter exceeds 100 percent.
    RetryJitterPercent,
    /// Exhaustion target is empty.
    RetryExhaustedTo,
    /// Exhaustion target is running.
    RetryRunningExhaustion,
    /// State name is empty.
    EmptyState,
    /// State name is running.
    RunningState,
    /// Retention duration is outside the source range.
    RetentionTtl,
    /// History count is outside the captured configuration range.
    RetentionHistory {
        /// Original admitted positive configuration limit.
        maximum: u32,
    },
    /// Indexed attribute trims to empty.
    IndexedAttributeEmpty,
    /// Indexed attribute has the reserved trimmed prefix.
    IndexedAttributeReserved,
    /// Indexed attribute is not UTF-8.
    IndexedAttributeUtf8,
    /// Nonempty state-metadata input trims to empty.
    IndexedStateMetaEmpty,
    /// State-metadata key has the reserved trimmed prefix.
    IndexedStateMetaReserved,
    /// State-metadata key is not UTF-8.
    IndexedStateMetaUtf8,
    /// A governance rule name is empty.
    GovernanceEmptyName(GovernanceRuleKind),
    /// A reached governance list element is empty.
    GovernanceEmptyList(GovernanceField),
    /// A limit exceeds the source exact integer range.
    GovernanceLimit,
    /// A positive field is zero or exceeds the source exact integer range.
    GovernancePositive(GovernanceField),
    /// A budget unit is empty.
    GovernanceUnit,
    /// Failure rate is outside one through 100.
    GovernancePercent,
    /// Circuit tracking fields violate the ordered source consistency rules.
    GovernanceTracking(CircuitTrackingFailure),
}

use CompletePolicyReplaceRejectionKind as Kind;

/// Immutable reached diagnosis, privately constructed by the content checker.
///
/// This value carries no account credit, Fresh origin or retained authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompletePolicyReplaceRejection {
    kind: Kind,
}

impl CompletePolicyReplaceRejection {
    /// Reads the immutable source reason.
    #[must_use]
    pub const fn kind(self) -> Kind {
        self.kind
    }

    /// Reads the original context limit only for a history rejection.
    #[must_use]
    pub const fn history_max_events_limit(self) -> Option<u32> {
        match self.kind {
            Kind::RetentionHistory { maximum } => Some(maximum),
            _ => None,
        }
    }

    /// Reads the existing collapsed schema diagnosis, without inferring origin.
    #[must_use]
    pub const fn schema_error(self) -> PolicySchemaError {
        match self.kind {
            Kind::EmptyType => PolicySchemaError::Policy(PolicyError::Domain(
                DomainError::EmptyValue(InputField::Type),
            )),
            Kind::InvalidVersion => PolicySchemaError::Policy(PolicyError::InvalidVersion),
            Kind::InvalidMaxActive => PolicySchemaError::Policy(PolicyError::InvalidMaxActive),
            kind @ (Kind::RetryMaxRetries
            | Kind::RetryBaseMillis
            | Kind::RetryMaxMillis
            | Kind::RetryJitterPercent
            | Kind::RetryExhaustedTo
            | Kind::RetryRunningExhaustion
            | Kind::EmptyState
            | Kind::RunningState) => retry_schema_error(kind),
            Kind::RetentionTtl => {
                PolicySchemaError::Policy(PolicyError::Retention(RetentionError::Ttl))
            }
            Kind::RetentionHistory { .. } => {
                PolicySchemaError::Policy(PolicyError::Retention(RetentionError::History))
            }
            Kind::IndexedAttributeEmpty
            | Kind::IndexedAttributeReserved
            | Kind::IndexedAttributeUtf8 => PolicySchemaError::InvalidIndexedAttributes,
            Kind::IndexedStateMetaEmpty
            | Kind::IndexedStateMetaReserved
            | Kind::IndexedStateMetaUtf8 => PolicySchemaError::InvalidIndexedStateMeta,
            Kind::GovernanceEmptyName(kind) => PolicySchemaError::InvalidName(kind),
            Kind::GovernanceEmptyList(field) => PolicySchemaError::InvalidListValue(field),
            Kind::GovernanceLimit => PolicySchemaError::InvalidInteger(GovernanceField::Limit),
            Kind::GovernancePositive(field) => PolicySchemaError::InvalidInteger(field),
            Kind::GovernanceUnit => PolicySchemaError::InvalidListValue(GovernanceField::Unit),
            Kind::GovernancePercent => PolicySchemaError::InvalidPercent,
            Kind::GovernanceTracking(failure) => PolicySchemaError::CircuitTracking(failure),
        }
    }
}

pub(super) fn reject(kind: Kind) -> Result<CompletePolicyReplaceRejection, PolicySchemaError> {
    let valid = match kind {
        Kind::RetentionHistory { maximum } => (1..=1_000_000).contains(&maximum),
        Kind::GovernanceEmptyList(field) => matches!(
            field,
            GovernanceField::Allowed
                | GovernanceField::Denied
                | GovernanceField::ApprovalEffects
                | GovernanceField::ApprovalStates
                | GovernanceField::Assignees
                | GovernanceField::ErrorClasses
        ),
        Kind::GovernancePositive(field) => matches!(
            field,
            GovernanceField::LeaseSize
                | GovernanceField::WindowMillis
                | GovernanceField::FailureThreshold
                | GovernanceField::OpenMillis
                | GovernanceField::MinCalls
                | GovernanceField::LatencyMillis
                | GovernanceField::HalfOpenProbes
                | GovernanceField::HalfOpenSuccesses
                | GovernanceField::ApprovalTimeout
        ),
        _ => true,
    };
    if !valid {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    Ok(CompletePolicyReplaceRejection { kind })
}

const fn retry_schema_error(kind: Kind) -> PolicySchemaError {
    let field = match kind {
        Kind::RetryMaxRetries => RetryPolicyField::MaxRetries,
        Kind::RetryBaseMillis => RetryPolicyField::BaseMillis,
        Kind::RetryMaxMillis => RetryPolicyField::MaxMillis,
        Kind::RetryJitterPercent => RetryPolicyField::JitterPercent,
        Kind::RetryExhaustedTo => RetryPolicyField::ExhaustedTo,
        Kind::RetryRunningExhaustion => RetryPolicyField::RunningExhaustion,
        Kind::EmptyState => RetryPolicyField::State,
        Kind::RunningState => RetryPolicyField::RunningState,
        _ => return PolicySchemaError::InvalidAuthority,
    };
    PolicySchemaError::Policy(PolicyError::Domain(DomainError::InvalidRetryPolicy(field)))
}

/// Emits exact source error bytes, freshly paying the received schema account.
///
/// A history body retains its original admitted limit. One output Vec is
/// reserved fallibly; the fixed seven-byte decimal scratch is not a native bound.
///
/// # Errors
/// Preserves profile/work/length diagnoses and selected reservation failure;
/// admitted work and earlier peak scratch persist on every refusal.
pub fn encode_complete_policy_replace_rejection_body(
    rejection: CompletePolicyReplaceRejection,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, CompletePolicyEncodingError> {
    use CompletePolicyEncodingError::Schema;

    if charge.profile() != limits {
        return Err(Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Schema)?;
    let prefix = replace_rejection_fixed_message(rejection.kind).map_err(Schema)?;
    let maximum = rejection.history_max_events_limit();
    let digits = maximum.map_or(0, decimal_length);
    let length = prefix
        .len()
        .checked_add(digits)
        .ok_or(Schema(PolicySchemaError::LengthOverflow))?;
    let digit_work =
        u64::try_from(digits).map_err(|_| Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, digit_work, 0).map_err(Schema)?;
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Schema)?;
    let mut scratch = [0; 7];
    let decimal = match maximum {
        Some(value) => write_replace_history_decimal(value, &mut scratch)?,
        None => &[],
    };
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
    output.extend_from_slice(prefix);
    output.extend_from_slice(decimal);
    Ok(output)
}

fn decimal_length(mut value: u32) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn write_replace_history_decimal(
    mut value: u32,
    scratch: &mut [u8; 7],
) -> Result<&[u8], CompletePolicyEncodingError> {
    let mut start = scratch.len();
    while value != 0 {
        start = start
            .checked_sub(1)
            .ok_or(CompletePolicyEncodingError::Schema(
                PolicySchemaError::LengthOverflow,
            ))?;
        let digit = u8::try_from(value % 10)
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        scratch[start] = b'0' + digit;
        value /= 10;
    }
    Ok(&scratch[start..])
}

fn replace_rejection_fixed_message(kind: Kind) -> Result<&'static [u8], PolicySchemaError> {
    Ok(match kind {
        Kind::EmptyType => b"ERR flow type must be a non-empty string",
        Kind::InvalidVersion => {
            b"ERR flow policy version must be a non-empty string or non-negative integer"
        }
        Kind::InvalidMaxActive => {
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity"
        }
        Kind::RetryMaxRetries => b"ERR flow retry max_retries must be between 0 and 1000",
        Kind::RetryBaseMillis => b"ERR flow retry base_ms must be between 0 and 2592000000",
        Kind::RetryMaxMillis => b"ERR flow retry max_ms must be between 0 and 2592000000",
        Kind::RetryJitterPercent => b"ERR flow retry jitter_pct must be between 0 and 100",
        Kind::RetryExhaustedTo => b"ERR flow retry exhausted_to must be a non-empty string",
        Kind::RetryRunningExhaustion => b"ERR flow retry exhausted_to cannot be running",
        Kind::EmptyState => b"ERR flow policy state must be a non-empty string",
        Kind::RunningState => b"ERR flow policy state cannot be running",
        Kind::RetentionTtl => b"ERR flow retention ttl_ms must be between 1 and 31536000000",
        Kind::RetentionHistory { .. } => {
            b"ERR flow retention history_max_events must be between 1 and "
        }
        Kind::IndexedAttributeEmpty => b"ERR flow attribute key must not be empty",
        Kind::IndexedAttributeReserved => b"ERR flow attribute key is reserved",
        Kind::IndexedAttributeUtf8 => b"ERR flow attribute key must be valid UTF-8",
        Kind::IndexedStateMetaEmpty => b"ERR flow state_meta key must not be empty",
        Kind::IndexedStateMetaReserved => b"ERR flow state_meta key is reserved",
        Kind::IndexedStateMetaUtf8 => b"ERR flow state_meta key must be valid UTF-8",
        kind => return governance_message(kind),
    })
}

fn governance_message(kind: Kind) -> Result<&'static [u8], PolicySchemaError> {
    Ok(match kind {
        Kind::GovernanceEmptyName(GovernanceRuleKind::Limits) => {
            b"ERR flow governance limits names must be non-empty strings"
        }
        Kind::GovernanceEmptyName(GovernanceRuleKind::Budgets) => {
            b"ERR flow governance budgets names must be non-empty strings"
        }
        Kind::GovernanceEmptyName(GovernanceRuleKind::Circuits) => {
            b"ERR flow governance circuits names must be non-empty strings"
        }
        Kind::GovernanceEmptyList(_) => {
            b"ERR flow governance list values must be non-empty strings"
        }
        Kind::GovernanceLimit => b"ERR flow governance limit must be a non-negative integer",
        Kind::GovernancePositive(field) => return positive_message(field),
        Kind::GovernanceUnit => {
            b"ERR flow governance unit must be a non-empty string of at most 65535 bytes"
        }
        Kind::GovernancePercent => {
            b"ERR flow governance failure_rate_pct must be an integer from 1 to 100"
        }
        Kind::GovernanceTracking(CircuitTrackingFailure::MinCallsAbove64) => {
            b"ERR flow governance circuit min_calls must be <= 64"
        }
        Kind::GovernanceTracking(CircuitTrackingFailure::ThresholdAbove64WithoutRate) => {
            b"ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct"
        }
        Kind::GovernanceTracking(CircuitTrackingFailure::MissingMinCallsForLargeThreshold) => {
            b"ERR flow governance circuit min_calls is required when failure_threshold exceeds 64"
        }
        _ => return Err(PolicySchemaError::InvalidAuthority),
    })
}

fn positive_message(field: GovernanceField) -> Result<&'static [u8], PolicySchemaError> {
    Ok(match field {
        GovernanceField::LeaseSize => b"ERR flow governance lease_size must be a positive integer",
        GovernanceField::WindowMillis => {
            b"ERR flow governance window_ms must be a positive integer"
        }
        GovernanceField::FailureThreshold => {
            b"ERR flow governance failure_threshold must be a positive integer"
        }
        GovernanceField::OpenMillis => b"ERR flow governance open_ms must be a positive integer",
        GovernanceField::MinCalls => b"ERR flow governance min_calls must be a positive integer",
        GovernanceField::LatencyMillis => {
            b"ERR flow governance latency_threshold_ms must be a positive integer"
        }
        GovernanceField::HalfOpenProbes => {
            b"ERR flow governance half_open_max_probes must be a positive integer"
        }
        GovernanceField::HalfOpenSuccesses => {
            b"ERR flow governance half_open_success_threshold must be a positive integer"
        }
        GovernanceField::ApprovalTimeout => {
            b"ERR flow governance timeout_ms must be a positive integer"
        }
        _ => return Err(PolicySchemaError::InvalidAuthority),
    })
}

pub(super) fn body_len(
    rejection: CompletePolicyReplaceRejection,
) -> Result<usize, PolicySchemaError> {
    let prefix = replace_rejection_fixed_message(rejection.kind)?;
    prefix
        .len()
        .checked_add(
            rejection
                .history_max_events_limit()
                .map_or(0, decimal_length),
        )
        .ok_or(PolicySchemaError::LengthOverflow)
}
