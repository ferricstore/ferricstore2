//! Operational pure preparation errors, separate from committed outcomes.

use std::{error::Error, fmt};

/// Failure before authoritative submission; no durable rejection is implied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationError {
    /// Invalid immutable limits.
    InvalidLimits,
    /// Missing versioned operation name.
    InvalidOperation,
    /// NaN or infinity is outside the comparison grammar.
    NonFiniteNumber,
    /// Repeated map key, including equal empty keys.
    DuplicateKey,
    /// Canonical byte budget, including root and prefix, exceeded.
    ByteLimit,
    /// Encoded value budget, including root and map keys, exceeded.
    ValueLimit,
    /// Recursive depth budget exceeded; root depth is one.
    DepthLimit,
    /// Checked count/byte/format-length arithmetic overflowed.
    LengthOverflow,
    /// Local fallible allocation failed; never a business rejection.
    Allocation,
    /// Unsupported comparison version; indicates a preparation invariant defect.
    UnsupportedVersion,
}

impl fmt::Display for PreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "invalid intent preparation limits",
            Self::InvalidOperation => "intent operation must be nonempty",
            Self::NonFiniteNumber => "intent requires finite binary64",
            Self::DuplicateKey => "intent map keys must be unique",
            Self::ByteLimit => "intent byte limit exceeded",
            Self::ValueLimit => "intent value limit exceeded",
            Self::DepthLimit => "intent depth limit exceeded",
            Self::LengthOverflow => "intent length overflow",
            Self::Allocation => "intent preparation allocation failed",
            Self::UnsupportedVersion => "unsupported intent version",
        })
    }
}

impl Error for PreparationError {}
