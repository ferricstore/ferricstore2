//! Pure source retention values; cleanup, references and storage are later gates.

use std::fmt;

/// Source maximum retention duration, one year in milliseconds.
pub const MAX_RETENTION_MILLIS: u64 = 31_536_000_000;
/// Source hard maximum retained history count.
pub const MAX_HISTORY_EVENTS: u32 = 1_000_000;

/// Public normalized fields; the internal hot-history cap is unrepresentable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetentionOverride {
    /// Optional positive terminal retention duration.
    pub ttl_ms: Option<u64>,
    /// Optional positive complete retained history bound.
    pub history_max_events: Option<u32>,
}

impl RetentionOverride {
    /// Checks source duration/history bounds using immutable apply configuration.
    ///
    /// # Errors
    /// Rejects zero, excessive TTL, or excessive configured history count.
    pub fn validate(self, maximum_events: u32) -> Result<(), RetentionError> {
        if self
            .ttl_ms
            .is_some_and(|value| value == 0 || value > MAX_RETENTION_MILLIS)
        {
            return Err(RetentionError::Ttl);
        }
        if self
            .history_max_events
            .is_some_and(|value| value == 0 || value > maximum_events.min(MAX_HISTORY_EVENTS))
        {
            return Err(RetentionError::History);
        }
        Ok(())
    }
}

/// Public resolved retention values, separate from internal retention mechanics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedRetention {
    /// Positive terminal retention duration.
    pub ttl_ms: u64,
    /// Bounded complete retained history count.
    pub history_max_events: u32,
}

impl Default for ResolvedRetention {
    fn default() -> Self {
        Self {
            ttl_ms: 604_800_000,
            history_max_events: 100_000,
        }
    }
}

/// Source field rejection; no retention cleanup plan is created here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionError {
    /// TTL must be between one millisecond and one year.
    Ttl,
    /// History count must fit its configured positive source bound.
    History,
}

impl fmt::Display for RetentionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ttl => "ERR flow retention ttl_ms must be between 1 and 31536000000",
            Self::History => "ERR flow retention history_max_events is outside configured bounds",
        })
    }
}
impl std::error::Error for RetentionError {}

pub(crate) fn merge(policy: &mut ResolvedRetention, override_: RetentionOverride) {
    if let Some(ttl) = override_.ttl_ms {
        policy.ttl_ms = ttl;
    }
    if let Some(events) = override_.history_max_events {
        policy.history_max_events = events;
    }
}
