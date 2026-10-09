//! Pure command-value validation and deterministic single-record rejections.

use std::fmt;

/// A business rejection; no record or history write accompanies an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// The target record is absent or has a different identity.
    NotFound,
    /// Ordinary create cannot replace an existing record.
    AlreadyExists,
    /// The supplied expected lifecycle state differs.
    WrongState,
    /// The supplied expected record version differs.
    WrongVersion,
    /// The current lease or fence does not authorize the operation.
    StaleLease,
    /// A terminal record requires the unimplemented rewind operation.
    Terminal,
    /// Running can only be entered through claim.
    RunningTransition,
    /// A transition cannot replace a dedicated terminal operation.
    TerminalTransition,
    /// A required binary value is empty.
    EmptyValue(InputField),
    /// Client-compatible committed time exceeds the source numeric ceiling.
    NowTooLarge,
    /// A run deadline exceeds the source numeric ceiling.
    RunAtTooLarge,
    /// An optimistic expected version exceeds the source numeric ceiling.
    ExpectedVersionTooLarge,
    /// A caller-supplied fencing guard exceeds the source numeric ceiling.
    FenceTooLarge,
    /// A lease duration is zero.
    ZeroLease,
    /// A lease duration exceeds the source numeric ceiling.
    LeaseTooLarge,
    /// Addition would exceed the source deadline ceiling or overflow.
    LeaseDeadlineTooLarge,
    /// A Rust counter would overflow; this is an added safety guard.
    CounterOverflow(Counter),
    /// The internal schedule workflow identity is outside this subset.
    ReservedId,
    /// The internal schedule workflow type is outside this subset.
    ReservedType,
    /// A normalized retry policy field violates the source bounds.
    InvalidRetryPolicy(RetryPolicyField),
    /// The authoritative policy view belongs to another workflow type.
    PolicyTypeMismatch,
    /// A generated retry deadline does not fit the explicit Rust representation.
    RetryDeadlineOverflow,
    /// A command has not been implemented in this increment.
    UnsupportedOperation,
}

/// Required binary command fields, preserving the source field names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputField {
    /// Workflow type.
    Type,
    /// Initial or claim state.
    State,
    /// Transition's expected source state.
    From,
    /// Transition's target state.
    To,
    /// Claim's worker identity.
    Worker,
    /// Caller-supplied opaque lease token.
    LeaseToken,
    /// Optional cancellation state guard.
    ExpectState,
}

/// Checked monotonic counters in the reduced record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Counter {
    /// Record version.
    Version,
    /// Lease fencing generation.
    Fence,
    /// Retry attempts.
    Attempts,
}

/// Source field-specific retry-policy validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyField {
    /// Inclusive retry-count cap.
    MaxRetries,
    /// Base backoff milliseconds.
    BaseMillis,
    /// Maximum backoff milliseconds.
    MaxMillis,
    /// Jitter percentage.
    JitterPercent,
    /// Empty exhaustion state.
    ExhaustedTo,
    /// `running` cannot be an exhaustion target.
    RunningExhaustion,
    /// Installed state policy requires a nonempty binary name.
    State,
    /// `running` cannot have a logical-state policy.
    RunningState,
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotFound => "ERR flow not found",
            Self::AlreadyExists => "ERR flow already exists",
            Self::WrongState => "ERR flow wrong state",
            Self::WrongVersion => "ERR flow changed concurrently",
            Self::StaleLease => "ERR stale flow lease",
            Self::Terminal => "ERR flow is terminal; use FLOW.REWIND",
            Self::RunningTransition => "ERR flow running state is only entered by FLOW.CLAIM_DUE",
            Self::TerminalTransition => {
                "ERR terminal flow state requires FLOW.COMPLETE, FLOW.FAIL, or FLOW.CANCEL"
            }
            Self::EmptyValue(field) => match field {
                InputField::Type => "ERR flow type must be a non-empty string",
                InputField::State => "ERR flow state must be a non-empty string",
                InputField::From => "ERR flow from must be a non-empty string",
                InputField::To => "ERR flow to must be a non-empty string",
                InputField::Worker => "ERR flow worker must be a non-empty string",
                InputField::LeaseToken => "ERR flow lease_token must be a non-empty string",
                InputField::ExpectState => "ERR flow expect_state must be a non-empty string",
            },
            Self::NowTooLarge => "ERR flow now_ms exceeds maximum 9007199254740991",
            Self::RunAtTooLarge => "ERR flow run_at_ms exceeds maximum 9007199254740991",
            Self::ExpectedVersionTooLarge => {
                "ERR flow expected_version exceeds maximum 9007199254740991"
            }
            Self::FenceTooLarge => "ERR flow fencing_token exceeds maximum 9007199254740991",
            Self::ZeroLease => "ERR flow lease_ms must be a positive integer",
            Self::LeaseTooLarge => "ERR flow lease_ms exceeds maximum 9007199254740991",
            Self::LeaseDeadlineTooLarge => {
                "ERR flow lease_ms deadline exceeds maximum 9007199254740991"
            }
            Self::CounterOverflow(counter) => match counter {
                Counter::Version => "ERR flow version overflow",
                Counter::Fence => "ERR flow fencing token overflow",
                Counter::Attempts => "ERR flow attempts overflow",
            },
            Self::ReservedId => "ERR flow id is reserved for internal use",
            Self::ReservedType => "ERR flow type is reserved for internal use",
            Self::InvalidRetryPolicy(field) => match field {
                RetryPolicyField::MaxRetries => {
                    "ERR flow retry max_retries must be between 0 and 1000"
                }
                RetryPolicyField::BaseMillis => {
                    "ERR flow retry base_ms must be between 0 and 2592000000"
                }
                RetryPolicyField::MaxMillis => {
                    "ERR flow retry max_ms must be between 0 and 2592000000"
                }
                RetryPolicyField::JitterPercent => {
                    "ERR flow retry jitter_pct must be between 0 and 100"
                }
                RetryPolicyField::ExhaustedTo => {
                    "ERR flow retry exhausted_to must be a non-empty string"
                }
                RetryPolicyField::RunningExhaustion => {
                    "ERR flow retry exhausted_to cannot be running"
                }
                RetryPolicyField::State => "ERR flow policy state must be a non-empty string",
                RetryPolicyField::RunningState => "ERR flow policy state cannot be running",
            },
            Self::PolicyTypeMismatch => "ERR flow retry policy type does not match record",
            Self::RetryDeadlineOverflow => "ERR flow retry deadline overflow",
            Self::UnsupportedOperation => "ERR unsupported single-record flow operation",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for DomainError {}
