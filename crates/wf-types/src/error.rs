//! Checked primitive construction errors, separate from workflow rejections.

use std::{error::Error, fmt};

use crate::bounds::{MAX_API_KEY_BYTES, MAX_RECORD_ID_BYTES};

/// An invalid binary value or an unrepresentable encoded length.
///
/// Empty-ID and oversized API-key display messages preserve the source API.
/// Other variants describe Rust primitive validation, not legacy wire errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueError {
    /// A record identifier was empty.
    EmptyRecordId,
    /// A record identifier exceeded the storage identity guard.
    RecordIdTooLong {
        /// Actual identifier byte length.
        bytes: usize,
    },
    /// An already encoded API key exceeded the source key-size limit.
    ApiKeyTooLong {
        /// Actual composed key byte length.
        bytes: usize,
    },
    /// Partition encoding received no components.
    EmptyPartitionComponents,
    /// Summing encoded byte lengths overflowed `usize`.
    LengthOverflow,
    /// The partition-key buffer could not reserve its checked length.
    AllocationFailed,
    /// A tenant/group/recovery/request/partition identity was empty.
    EmptyScopeId,
    /// A scoped identity exceeded its declared byte bound.
    ScopeIdTooLong {
        /// Actual opaque identity length.
        bytes: usize,
        /// Maximum admitted length for this identity kind.
        limit: usize,
    },
    /// Committed physical time exceeded the source 48-bit clock bound.
    PhysicalTimeTooLarge,
    /// Committed wall time exceeded its paired physical clock.
    WallTimeAfterPhysical,
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRecordId => formatter.write_str("ERR flow id must be a non-empty string"),
            Self::RecordIdTooLong { .. } => formatter.write_str(&format!(
                "record id exceeds storage identity limit (max {MAX_RECORD_ID_BYTES} bytes)"
            )),
            Self::ApiKeyTooLong { .. } => formatter.write_str(&format!(
                "ERR key too large (max {MAX_API_KEY_BYTES} bytes)"
            )),
            Self::EmptyPartitionComponents => {
                formatter.write_str("partition key requires at least one binary component")
            }
            Self::LengthOverflow => formatter.write_str("encoded byte length overflow"),
            Self::AllocationFailed => formatter.write_str("partition key allocation failed"),
            Self::EmptyScopeId => formatter.write_str("scope identity must be nonempty"),
            Self::ScopeIdTooLong { limit, .. } => {
                formatter.write_str(&format!("scope identity exceeds {limit} bytes"))
            }
            Self::PhysicalTimeTooLarge => {
                formatter.write_str("physical clock exceeds 48-bit bound")
            }
            Self::WallTimeAfterPhysical => {
                formatter.write_str("wall clock exceeds committed physical time")
            }
        }
    }
}

impl Error for ValueError {}
