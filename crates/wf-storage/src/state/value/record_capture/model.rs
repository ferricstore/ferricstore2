//! Explicit borrowed inputs and independent logical resource limits.

use super::super::{ScannedWorkflowRecord, StateValueError};
use std::fmt;
use wf_domain::metadata::record::PreparedOriginalRecordMetadata;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};

/// Complete proposed data, without storage or original-source authority.
#[derive(Clone, Copy)]
pub struct RecordCaptureInput<'a> {
    /// Exact native group/recovery/tenant scope.
    pub scope: &'a RequestScope,
    /// Actual physical partition; never inferred from the logical metadata.
    pub partition: &'a PartitionId,
    /// Existing immutable core record.
    pub record: &'a WorkflowRecord,
    /// Actual core ordering metadata.
    pub ordering: OrderingMetadata,
    /// Explicit original birth sequence, preserved across later mutations.
    pub incarnation: u128,
    /// Explicit metadata operand; absence cannot become an empty capture.
    pub metadata: &'a PreparedOriginalRecordMetadata,
}

/// Exact expected stored association, supplied by the authoritative reader.
#[derive(Clone, Copy)]
pub struct RecordCapturePoint<'a> {
    /// Exact native namespace.
    pub scope: &'a RequestScope,
    /// Exact physical partition.
    pub partition: &'a PartitionId,
    /// Exact record identity.
    pub id: &'a RecordId,
    /// Expected original birth sequence.
    pub incarnation: u128,
    /// Expected core version, not merely an upper bound.
    pub version: u64,
}

/// Separate prospective logical limits; allocator and engine pages are excluded.
#[derive(Clone, Copy)]
pub struct RecordCaptureLimits {
    /// Existing core semantics and field/record admission.
    pub admission: AdmissionLimits,
    /// Complete envelope, including framing and checksum.
    pub maximum_encoded_bytes: usize,
    /// Complete opaque original metadata operand.
    pub maximum_metadata_bytes: usize,
    /// Conservative 4096 plus 128 times complete encoded bytes.
    pub maximum_work_units: usize,
    /// Encoder: 4096 plus final requested bytes; scanner: fixed 4096.
    pub maximum_temporary_bytes: usize,
}

/// Borrowed core and metadata from one encoded donor, with no typed ETF promise.
pub struct ScannedRecordCapture<'a> {
    pub(super) core: ScannedWorkflowRecord<'a>,
    pub(super) core_bytes: &'a [u8],
    pub(super) metadata: &'a [u8],
    pub(super) incarnation: u128,
    pub(super) work: usize,
}
impl<'a> ScannedRecordCapture<'a> {
    /// Borrows the validated existing core view.
    #[must_use]
    pub const fn core(&self) -> &ScannedWorkflowRecord<'a> {
        &self.core
    }
    /// Borrows the unchanged complete FWSR2 core snapshot.
    #[must_use]
    pub const fn core_bytes(&self) -> &'a [u8] {
        self.core_bytes
    }
    /// Borrows opaque original metadata; use its decoder with explicit context.
    #[must_use]
    pub const fn metadata_bytes(&self) -> &'a [u8] {
        self.metadata
    }
    /// Reads the validated original birth identity.
    #[must_use]
    pub const fn incarnation(&self) -> u128 {
        self.incarnation
    }
    /// Reads the conservative charged work for this complete scan.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
}
impl fmt::Debug for ScannedRecordCapture<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScannedRecordCapture(<redacted>)")
    }
}

/// Resource units remain distinct from existing core/framing failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordCaptureError {
    /// A required independent limit is zero.
    InvalidLimits,
    /// Existing core, association, framing or encoded-byte failure.
    State(StateValueError),
    /// Conservative work exceeds the supplied account.
    WorkLimit,
    /// Simultaneous logical ownership exceeds the supplied ceiling.
    TemporaryLimit,
}
impl From<StateValueError> for RecordCaptureError {
    fn from(value: StateValueError) -> Self {
        Self::State(value)
    }
}
impl fmt::Display for RecordCaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid record capture limits"),
            Self::State(error) => error.fmt(f),
            Self::WorkLimit => f.write_str("record capture work limit"),
            Self::TemporaryLimit => f.write_str("record capture temporary limit"),
        }
    }
}
impl std::error::Error for RecordCaptureError {}
