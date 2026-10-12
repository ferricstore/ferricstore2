//! Explicit committed time and fencing values with nonwrapping arithmetic.

use crate::error::ValueError;

mod position;
pub use position::{AppliedPosition, LogIdentity, Revision};

/// Milliseconds explicitly supplied by the committed command or its caller.
///
/// Zero is valid. This value does not read a clock, enforce global monotonic
/// time, or decide whether a requested deadline is admissible.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimestampMillis(u64);

impl TimestampMillis {
    /// Preserve an explicit millisecond value, including zero.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the explicit millisecond value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Add milliseconds, returning `None` if the deadline would overflow.
    pub fn checked_add(self, milliseconds: u64) -> Option<Self> {
        self.0.checked_add(milliseconds).map(Self)
    }
}

/// A monotonic fencing counter supplied by authoritative workflow state.
///
/// Zero preserves the source initial value. Namespace and stale-owner checks
/// belong to the domain and durable adapters, not to this primitive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fence(u64);

impl Fence {
    /// Preserve an explicit fencing value, including zero.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the authoritative counter value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advance exactly once, returning `None` rather than wrapping at `u64::MAX`.
    pub fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

/// Validated source physical/logical committed command clock.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HlcTimestamp {
    physical: TimestampMillis,
    logical: u16,
}

impl HlcTimestamp {
    /// Preserves an explicit clock snapshot without generating time.
    ///
    /// # Errors
    /// Rejects physical milliseconds above the source 48-bit ceiling.
    pub const fn new(physical: u64, logical: u16) -> Result<Self, ValueError> {
        if physical >= (1_u64 << 48) {
            Err(ValueError::PhysicalTimeTooLarge)
        } else {
            Ok(Self {
                physical: TimestampMillis::new(physical),
                logical,
            })
        }
    }

    /// Reads committed physical milliseconds.
    #[must_use]
    pub const fn physical(self) -> TimestampMillis {
        self.physical
    }

    /// Reads the committed logical component.
    #[must_use]
    pub const fn logical(self) -> u16 {
        self.logical
    }
}

/// Validated committed HLC/wall pair, supplied by command preparation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommittedTime {
    hlc: HlcTimestamp,
    wall: TimestampMillis,
}

impl CommittedTime {
    /// Preserves committed metadata; no wall clock is consulted.
    ///
    /// # Errors
    /// Rejects wall milliseconds above the paired physical snapshot.
    pub const fn new(hlc: HlcTimestamp, wall: u64) -> Result<Self, ValueError> {
        if wall > hlc.physical.get() {
            Err(ValueError::WallTimeAfterPhysical)
        } else {
            Ok(Self {
                hlc,
                wall: TimestampMillis::new(wall),
            })
        }
    }

    /// Reads the committed hybrid clock.
    #[must_use]
    pub const fn hlc(self) -> HlcTimestamp {
        self.hlc
    }

    /// Reads physical eligibility time, distinct from logical clock progression.
    #[must_use]
    pub const fn wall(self) -> TimestampMillis {
        self.wall
    }
}
