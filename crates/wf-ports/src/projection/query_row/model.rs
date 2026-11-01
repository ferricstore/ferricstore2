//! Ordinary captured data and prepared bytes, without provenance or authority.
use wf_domain::metadata::OriginalMetadataInput;

/// Positive operational ceilings, distinct from original semantic limits.
#[derive(Clone, Copy, Debug)]
pub struct OriginalQueryRecordLimits {
    /// Maximum complete returned bytes (metadata plus scope, or complete row).
    pub maximum_output_bytes: usize,
    /// Maximum continuous validation/encoding/checksum work.
    pub maximum_work_units: usize,
    /// Conservative requested logical peak, excluding borrowed caller input.
    pub maximum_temporary_bytes: usize,
}

/// Captured original Locator-class fields; acquisition remains caller-owned.
/// The field map is ordinary data and grants no physical availability proof.
#[derive(Clone, Copy, Debug)]
pub struct OriginalQueryLocator<'rows, 'leaf> {
    /// All original struct fields, addressed by atom keys; absent fields are nil.
    pub fields: OriginalMetadataInput<'rows, 'leaf>,
}

/// Original business errors and independent operational refusal.
#[derive(Debug)]
pub enum OriginalQueryRecordError {
    /// Original invalid-query-metadata error (also invalid row/locator data).
    InvalidRecord,
    /// Original metadata exceeds32629 bytes.
    MetadataTooLarge,
    /// An operational ceiling is zero.
    InvalidLimits,
    /// A checked operational ceiling is exceeded.
    Limit,
    /// A fallible reservation failed.
    ReserveFailed(std::collections::TryReserveError),
    /// An operational scope preparation error.
    Scope(wf_domain::metadata::scope::OriginalScopeError),
}
impl OriginalQueryRecordError {
    /// Exact source `validate_record` error; operational failures have no source error.
    #[must_use]
    pub const fn source_error(&self) -> Option<&'static str> {
        match self {
            Self::InvalidRecord => Some("ERR invalid flow query metadata"),
            Self::MetadataTooLarge => Some("ERR flow query metadata exceeds 32629 bytes"),
            _ => None,
        }
    }
}

/// Prepared original metadata and source identity as ordinary data.
pub struct PreparedOriginalQueryMetadata<'a> {
    pub(super) encoded: Vec<u8>,
    pub(super) scope: Option<Vec<u8>>,
    pub(super) id: &'a [u8],
    pub(super) partition: Option<&'a [u8]>,
    pub(super) version: u64,
    pub(super) work: usize,
    pub(super) temporary: usize,
}
impl<'a> PreparedOriginalQueryMetadata<'a> {
    /// Exact original encoded metadata section.
    #[must_use]
    pub fn encoded_metadata(&self) -> &[u8] {
        &self.encoded
    }
    /// Exact derived isolation scope, when present.
    #[must_use]
    pub fn scope_prefix(&self) -> Option<&[u8]> {
        self.scope.as_deref()
    }
    /// Original run identity, borrowing the captured input.
    #[must_use]
    pub const fn id(&self) -> &'a [u8] {
        self.id
    }
    /// Original logical partition after public projection.
    #[must_use]
    pub const fn logical_partition(&self) -> Option<&'a [u8]> {
        self.partition
    }
    /// Original logical version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    /// Continuous charged work.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Conservative requested logical peak, not allocator/RSS measurement.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}

/// One complete encoded query row, without durable publication authority.
pub struct PreparedOriginalQueryRow {
    pub(super) encoded: Vec<u8>,
    pub(super) work: usize,
    pub(super) temporary: usize,
}
impl PreparedOriginalQueryRow {
    /// Borrow the original encoded physical query row.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    /// Move the same complete byte owner without copying.
    #[must_use]
    pub fn into_encoded(self) -> Vec<u8> {
        self.encoded
    }
    /// Continuous charged work.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Conservative requested logical peak, not allocator/RSS measurement.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}

impl std::fmt::Debug for PreparedOriginalQueryMetadata<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("PreparedOriginalQueryMetadata(<redacted>)")
    }
}
impl std::fmt::Debug for PreparedOriginalQueryRow {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("PreparedOriginalQueryRow(<redacted>)")
    }
}
