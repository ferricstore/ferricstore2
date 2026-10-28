//! Captured snapshots and flat output owners without authority or hidden state.

use std::{borrow::Cow, collections::TryReserveError, fmt};

use crate::history::metadata::OriginalHistoryMetadataField;
use crate::metadata::OriginalMetadataInput;

/// Already resolved source binary context; absence falls back to empty text.
#[derive(Clone, Copy, Debug, Default)]
pub struct OriginalCompactHistoryContext<'a> {
    /// Workflow identity omitted from the encoded event.
    pub id: Option<&'a [u8]>,
    /// Workflow type omitted from the encoded event.
    pub workflow_type: Option<&'a [u8]>,
    /// Parent identity, without backlink validation.
    pub parent_flow_id: Option<&'a [u8]>,
    /// Root identity, without lineage validation.
    pub root_flow_id: Option<&'a [u8]>,
    /// Optional opaque correlation bytes.
    pub correlation_id: Option<&'a [u8]>,
}

/// Explicit original snapshot; integer captures retain arbitrary magnitudes.
///
/// Noninteger numeric classes use the original nil primitive. Compound classes
/// are not traversed. The context is captured but omitted from encoded FSH2.
pub struct OriginalCompactHistorySnapshot<'rows, 'leaf> {
    /// Actual binary event label, including empty bytes.
    pub event: &'leaf [u8],
    /// Source record version, without mutation or identity validation.
    pub version: OriginalMetadataInput<'rows, 'leaf>,
    /// Explicit source event time, without reading a clock.
    pub at: OriginalMetadataInput<'rows, 'leaf>,
    /// Binary state; absence uses the nil primitive.
    pub state: Option<&'leaf [u8]>,
    /// Priority, attempts, fence, created, updated, next-run and lease-deadline.
    pub numbers: [OriginalMetadataInput<'rows, 'leaf>; 7],
    /// Owner, payload, result, error and rewound-event reference bytes.
    pub references: [Option<&'leaf [u8]>; 5],
    /// Immutable snapshot data, deliberately absent from the emitted event.
    pub context: OriginalCompactHistoryContext<'leaf>,
    /// Already normalized ordered pairs, including duplicate normalized aliases.
    pub metadata: &'rows [OriginalHistoryMetadataField<'leaf>],
}

/// Independent positive logical ceilings, without native allocation guarantees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalCompactHistoryLimits {
    /// Actual borrowed input occurrences and descriptor spines.
    pub max_input_bytes: usize,
    /// Encoded or decoded metadata pairs before filtering.
    pub max_metadata_pairs: usize,
    /// Complete encoded output or decoder input bytes.
    pub max_encoded_bytes: usize,
    /// Complete decoded field key/value bytes, counting repeated occurrences.
    pub max_output_bytes: usize,
    /// Continuous validation, measurement, emission and rendering work.
    pub max_work_units: usize,
    /// Simultaneous requested logical owners and named fixed control.
    pub max_temporary_bytes: usize,
}

/// Complete FSH2 owner, distinct from stored mutation-history points.
pub struct PreparedOriginalCompactHistory {
    pub(in crate::history::compact) bytes: Vec<u8>,
    pub(in crate::history::compact) work: usize,
    pub(in crate::history::compact) temporary: usize,
}

impl PreparedOriginalCompactHistory {
    /// Borrow the complete encoded event, including its magic.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Move the original buffer without copying or shrinking.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Continuous work through successful preparation.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }

    /// Peak prospectively requested logical ownership.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}

impl fmt::Debug for PreparedOriginalCompactHistory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedOriginalCompactHistory(<redacted>)")
    }
}

/// One protocol pair; a nil event remains distinct from an empty binary event.
#[derive(Debug)]
pub struct OriginalCompactHistoryField<'a> {
    /// Static base key or borrowed metadata key.
    pub key: Cow<'a, [u8]>,
    /// Borrowed binary, owned decimal text, or the source nil event.
    pub value: Option<Cow<'a, [u8]>>,
}

/// Ordered base and metadata fields, without a recursive owner or read authority.
pub struct DecodedOriginalCompactHistory<'a> {
    pub(in crate::history::compact) fields: Vec<OriginalCompactHistoryField<'a>>,
    pub(in crate::history::compact) work: usize,
    pub(in crate::history::compact) temporary: usize,
}

impl<'a> DecodedOriginalCompactHistory<'a> {
    /// Ordered pairs, including duplicate metadata keys and any nil event.
    #[must_use]
    pub fn fields(&self) -> &[OriginalCompactHistoryField<'a>] {
        &self.fields
    }

    /// Move the same flat spine and numeric owners without cloning.
    #[must_use]
    pub fn into_fields(self) -> Vec<OriginalCompactHistoryField<'a>> {
        self.fields
    }

    /// Continuous work through validation and complete rendering.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }

    /// Peak prospectively requested logical ownership.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}

impl fmt::Debug for DecodedOriginalCompactHistory<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DecodedOriginalCompactHistory(<redacted>)")
    }
}

/// Operational/capture errors, separate from the source's empty corrupt decode.
#[derive(Debug)]
pub enum OriginalCompactHistoryError {
    /// A required positive ceiling is zero.
    InvalidLimits,
    /// A selected capture is inconsistent with the declared input contract.
    InvalidInput,
    /// A checked resource overflows or exceeds its independent ceiling.
    Limit,
    /// An actual fallible variable reservation failed.
    ReserveFailed(TryReserveError),
}

impl fmt::Display for OriginalCompactHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid compact history limits"),
            Self::InvalidInput => formatter.write_str("invalid compact history capture"),
            Self::Limit => formatter.write_str("compact history resource limit"),
            Self::ReserveFailed(error) => write!(formatter, "compact history reserve: {error}"),
        }
    }
}

impl std::error::Error for OriginalCompactHistoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReserveFailed(error) => Some(error),
            Self::InvalidLimits | Self::InvalidInput | Self::Limit => None,
        }
    }
}
