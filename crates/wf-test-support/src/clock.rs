//! Explicit, shared, deterministic clock input for future source-derived tests.

use std::{
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Rejected manual clock changes leave its value unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockError {
    /// The requested timestamp precedes the current injected timestamp.
    Backwards,
    /// Advancing the injected timestamp would overflow its representation.
    Overflow,
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backwards => f.write_str("injected clock cannot move backwards"),
            Self::Overflow => f.write_str("injected clock overflow"),
        }
    }
}

impl Error for ClockError {}

/// A shared clock whose only source of progress is an explicit test action.
///
/// No workflow port is implemented in M0. Later adapters must pass this value
/// as input, rather than introduce system time into a deterministic reducer.
#[derive(Clone, Debug)]
pub struct ManualClock {
    millis: Arc<AtomicU64>,
}

impl ManualClock {
    /// Start at the supplied timestamp, including zero.
    #[must_use]
    pub fn new(millis: u64) -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(millis)),
        }
    }

    /// Read the last explicitly injected timestamp.
    #[must_use]
    pub fn now_millis(&self) -> u64 {
        self.millis.load(Ordering::SeqCst)
    }

    /// Advance atomically, returning the timestamp from this change.
    ///
    /// # Errors
    /// Returns [`ClockError::Overflow`] without modifying the clock on overflow.
    pub fn advance_by(&self, millis: u64) -> Result<u64, ClockError> {
        let prior = self
            .millis
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_add(millis)
            })
            .map_err(|_| ClockError::Overflow)?;
        prior.checked_add(millis).ok_or(ClockError::Overflow)
    }

    /// Set a nondecreasing timestamp atomically; equality is permitted.
    ///
    /// # Errors
    /// Returns [`ClockError::Backwards`] without changing the clock on rejection.
    pub fn set_millis(&self, millis: u64) -> Result<(), ClockError> {
        self.millis
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |prior| {
                (millis >= prior).then_some(millis)
            })
            .map(|_| ())
            .map_err(|_| ClockError::Backwards)
    }
}
