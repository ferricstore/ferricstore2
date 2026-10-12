//! Borrowed public caller comparison, separate from claim admission and authority.
//!
//! These values preserve mathematical caller distinctions. They do not establish
//! tenant scope, configured source limits, committed time or claim eligibility.

use std::fmt;

mod input;
mod normalize;
mod normalized;

pub use input::{
    ClaimCallerLimits, ClaimDueInput, ClaimPartitionMode, ClaimPartitionsInput, ClaimReplyMode,
    ClaimStatesInput, ClaimValuesInput, ClaimValuesMode,
};
pub use normalize::normalize_claim_due;
pub use normalized::NormalizedClaimDue;

/// Preparation resource failure, without a durable business outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimCallerError {
    /// A selected comparison profile is zero or exceeds its scalar ceiling.
    InvalidLimits,
    /// One raw list exceeds the configured occurrence count.
    ComponentCount,
    /// An actual caller binary exceeds the component ceiling.
    ValueTooLarge {
        /// Actual binary length.
        bytes: usize,
        /// Configured component ceiling.
        maximum: usize,
    },
    /// Actual occurrence bytes exceed the aggregate preparation profile.
    RawBytes {
        /// Checked occurrence bytes at the failed aggregate check.
        bytes: usize,
        /// Configured aggregate ceiling.
        maximum: usize,
    },
    /// Actual occurrence-byte addition overflows its scalar representation.
    LengthOverflow,
    /// A requested reference-spine reservation failed.
    Allocation,
}

impl fmt::Display for ClaimCallerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "invalid claim caller comparison limits",
            Self::ComponentCount => "claim caller raw occurrence count exceeds limits",
            Self::ValueTooLarge { .. } => "claim caller binary exceeds component limits",
            Self::RawBytes { .. } => "claim caller occurrence bytes exceed aggregate limits",
            Self::LengthOverflow => "claim caller occurrence-byte length overflow",
            Self::Allocation => "claim caller reference-spine allocation failed",
        })
    }
}

impl std::error::Error for ClaimCallerError {}
