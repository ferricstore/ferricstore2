//! Finite operation bounds and one owned spine of borrowed or converted text.

use std::borrow::Cow;
use std::collections::TryReserveError;
use std::fmt;

/// Independent positive admission ceilings for captured history metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalHistoryMetadataLimits {
    /// Complete borrowed byte pool plus supplied node and edge spines.
    pub max_input_bytes: usize,
    /// Maximum root map rows before filtering unsupported classes.
    pub max_pairs: usize,
    /// Exact sum of returned key and value byte lengths, including aliases.
    pub max_output_bytes: usize,
    /// Continuous control, validation, conversion and copy work.
    pub max_work_units: usize,
    /// Simultaneous requested output/scratch owners and fixed logical control.
    pub max_temporary_bytes: usize,
}

/// One source field, retaining borrowed bytes or its owned scalar conversion.
pub struct OriginalHistoryMetadataField<'a> {
    /// Normalized atom/binary key, without merging equal normalized aliases.
    pub key: Cow<'a, [u8]>,
    /// Binary data or normalized scalar text; source nil becomes empty bytes.
    pub value: Cow<'a, [u8]>,
}

/// One normalized ordered field spine, without capture or storage authority.
pub struct PreparedOriginalHistoryMetadata<'a> {
    pub(in crate::history::metadata) fields: Vec<OriginalHistoryMetadataField<'a>>,
    pub(in crate::history::metadata) work: usize,
    pub(in crate::history::metadata) temporary: usize,
}

impl<'a> PreparedOriginalHistoryMetadata<'a> {
    /// Ordered fields; duplicate normalized keys remain distinct positions.
    #[must_use]
    pub fn fields(&self) -> &[OriginalHistoryMetadataField<'a>] {
        &self.fields
    }

    /// Move the same field spine and converted buffers without cloning.
    #[must_use]
    pub fn into_fields(self) -> Vec<OriginalHistoryMetadataField<'a>> {
        self.fields
    }

    /// Logical work charged through the complete successful operation.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }

    /// Peak named requested ownership, without a native RSS/frame/OOM promise.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}

impl fmt::Debug for PreparedOriginalHistoryMetadata<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedOriginalHistoryMetadata(<redacted>)")
    }
}

/// Operational/capture failures, separate from ignored source value classes.
#[derive(Debug)]
pub enum OriginalHistoryMetadataError {
    /// One or more required ceilings are zero.
    InvalidLimits,
    /// A selected capture has an invalid span, identity or scalar representation.
    InvalidInput,
    /// Checked resources overflowed or exceeded an independent ceiling.
    Limit,
    /// An actual fallible output or scratch reservation failed.
    ReserveFailed(TryReserveError),
}

impl fmt::Display for OriginalHistoryMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid history metadata limits"),
            Self::InvalidInput => formatter.write_str("invalid history metadata capture"),
            Self::Limit => formatter.write_str("history metadata resource limit"),
            Self::ReserveFailed(error) => write!(formatter, "history metadata reserve: {error}"),
        }
    }
}

impl std::error::Error for OriginalHistoryMetadataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReserveFailed(error) => Some(error),
            Self::InvalidLimits | Self::InvalidInput | Self::Limit => None,
        }
    }
}
