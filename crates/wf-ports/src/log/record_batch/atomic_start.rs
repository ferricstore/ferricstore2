//! Reserved atomic-start codec and retained-response interfaces.
//!
//! Encoding, fresh application and retained data replies are not implemented.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::{IndexedDecision, TouchedIndexedGroupView};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::consensus::PreparedRequest;

use super::super::{CodecError, CodecLimits, DecodedRequest, PreparedBytes};
use super::{FreshRecordBatch, RecordBatchApplyError, RecordBatchApplyPlan, RecordBatchInspection};

/// Reserves private atomic-start encoding without producing a frame.
/// Owned request inputs are dropped on refusal.
/// # Errors
/// Always returns `CodecError::UnsupportedTag`.
pub fn encode_atomic_start(
    _request: PreparedRequest,
    _limits: CodecLimits,
    _workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    Err(CodecError::UnsupportedTag.into())
}

/// Reserves borrowed atomic-start preparation without parsing or materializing.
/// # Errors
/// Always returns `CodecError::UnsupportedVersion`.
pub fn prepare_atomic_start(
    _encoded: &[u8],
    _limits: CodecLimits,
    _workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    Err(CodecError::UnsupportedVersion.into())
}

/// Reserves the original-outcome point gate without inspecting authority.
/// Owned decoded inputs are dropped; no fresh or retained result is produced.
/// # Errors
/// Always returns `CodecError::UnsupportedVersion`.
pub fn inspect_atomic_start_point<'input, 'retained>(
    _decoded: DecodedRequest<'input>,
    _log: LogIdentity,
    _view: PointGateView<'_, 'retained>,
    _limits: CodecLimits,
    _workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    Err(CodecError::UnsupportedVersion.into())
}

/// Reserves touched atomic-start application without producing a plan or reply.
/// Owned fresh inputs are dropped without looking up current records or policy.
/// # Errors
/// Always returns `CodecError::UnsupportedVersion`.
pub fn decide_fresh_atomic_start_touched<'current>(
    _fresh: FreshRecordBatch<'_>,
    _view: TouchedIndexedGroupView<'_, 'current>,
    _additional_live_bytes: usize,
    _outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    Err(CodecError::UnsupportedVersion.into())
}

/// Reserves retained atomic-start validation without accepting response bytes.
/// # Errors
/// Always returns `CodecError::UnsupportedVersion`.
pub fn atomic_start_retained_summary(
    _intent: ClientIntent,
    _response_version: u16,
    _response: &[u8],
    _original_position: LogIdentity,
    _original_time: TimestampMillis,
    _admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Reserves actual-plan response comparison without returning a match verdict.
/// # Errors
/// Always returns `CodecError::UnsupportedVersion`.
pub fn atomic_start_response_matches(
    _decision: &IndexedDecision<'_>,
    _response: &[u8],
    _original_position: LogIdentity,
    _original_time: TimestampMillis,
    _admission: AdmissionLimits,
) -> Result<bool, CodecError> {
    Err(CodecError::UnsupportedVersion)
}
