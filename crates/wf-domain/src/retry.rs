//! Retry inputs with explicit time, installed-policy overrides and jitter.

use wf_types::version::TimestampMillis;

use crate::lease::LeaseGuard;
use crate::policy::RetryOverride;

mod source;
pub use source::source_retry_jitter;

/// Source retry audit decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    /// The incremented attempt count still permits retrying the logical state.
    Scheduled,
    /// The retry limit was exceeded and the configured target was selected.
    Exhausted,
}

impl RetryDecision {
    /// Returns the exact source audit label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Exhausted => "exhausted",
        }
    }
}

/// Explicit committed hash sample used by source-compatible retry jitter.
///
/// Explicit-sample callers supply the pinned source tuple hash when needed.
/// The public mutation reducer separately derives it after authoritative guards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitterSample(u32);

impl JitterSample {
    /// Retains an explicit bounded sample without consulting a hash or RNG.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Reads the committed sample.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Retry using the authoritative installed policy and normalized command input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRetry {
    /// Current ownership guard, or fence for a legacy duplicate.
    pub guard: LeaseGuard,
    /// Explicit caller deadline; omission requests computed policy timing.
    pub run_at: Option<TimestampMillis>,
    /// Highest-precedence normalized retry fields.
    pub policy_override: RetryOverride,
    /// Explicit source-compatible sample; ignored when no jitter is needed.
    pub jitter: JitterSample,
    /// Optional replacement payload; omission preserves current payload.
    pub payload: Option<Vec<u8>>,
    /// Optional replacement error; omission clears current error.
    pub error: Option<Vec<u8>>,
}

/// A scheduled retry time, fully resolved before committed application.
///
/// This legacy caller-resolved path schedules only. [`PolicyRetry`] separately
/// resolves installed policy, computes jitter and decides exhaustion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryTime {
    /// The legacy caller explicitly supplied `run_at_ms`.
    Explicit(TimestampMillis),
    /// The caller omitted `run_at_ms`; a preparation layer resolved policy time.
    ///
    /// The resolved value is not used to match a legacy duplicate schedule.
    PolicyResolved(TimestampMillis),
}

impl RetryTime {
    /// Returns the committed deadline used only for a new retry mutation.
    #[must_use]
    pub const fn committed(self) -> TimestampMillis {
        match self {
            Self::Explicit(time) | Self::PolicyResolved(time) => time,
        }
    }
}

/// Release a running lease and schedule the next attempt in its logical state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Retry {
    /// Current lease and fence, or fence for a source-permitted legacy duplicate.
    pub guard: LeaseGuard,
    /// Explicit requested deadline or committed caller-resolved policy deadline.
    pub run_at: RetryTime,
    /// Optional new payload; omission retains the old payload.
    pub payload: Option<Vec<u8>>,
    /// Optional new error; omission clears the current error observation.
    pub error: Option<Vec<u8>>,
}
