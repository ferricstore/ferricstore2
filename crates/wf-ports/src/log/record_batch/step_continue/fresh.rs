//! Selected retained/Fresh Step interface; rejecting before genuine tests.

use wf_domain::workflow::indexed::TouchedIndexedGroupView;
use wf_domain::workflow::request::gate::PointGateView;
use wf_types::version::LogIdentity;

use super::super::{
    FreshRecordBatch, RecordBatchApplyError, RecordBatchApplyPlan, RecordBatchInspection,
};
use crate::log::{CodecError, CodecLimits, DecodedRequest};

/// Inspects one original retained outcome before current Step authority.
/// # Errors
/// Currently rejects every input until the tests-first composition is implemented.
pub fn inspect_step_continue_point<'input, 'retained>(
    _decoded: DecodedRequest<'input>,
    _log: LogIdentity,
    _view: PointGateView<'_, 'retained>,
    _limits: CodecLimits,
    _workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    Err(CodecError::UnsupportedTag.into())
}

/// Consumes Fresh Step inputs into a proposal and original response, without commit.
/// # Errors
/// Currently rejects every input until the tests-first composition is implemented.
pub fn decide_fresh_step_continue_touched<'current>(
    _fresh: FreshRecordBatch<'_>,
    _view: TouchedIndexedGroupView<'_, 'current>,
    _additional_live_bytes: usize,
    _outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    Err(CodecError::UnsupportedTag.into())
}
