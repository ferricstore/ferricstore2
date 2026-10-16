//! Verified V6 preparation, one point gate and an owned counted Fresh response.

use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::indexed::{EntryOrder, TouchedIndexedGroupView};
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
use wf_domain::workflow::request::{RequestValueError, RetainedMetadata, RetainedOutcome};
use wf_types::version::LogIdentity;

use crate::consensus::{PreparedOperation, PreparedRequest};
use crate::log::{CodecError, CodecLimits, DecodedRequest, PreparedBytes};

use super::phase::finish_inspection;
use super::{
    FreshRecordBatch, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan,
    RecordBatchInspection, admit, check_decoded, encode_profile, prepare_profile, response, sum,
};

/// Encodes eight independent mutations with their actual V6 caller-field digest.
/// Public normalization/submission and persistence authority remain external.
/// # Errors
/// Rejects unsupported operations, framing, admission or prospective workspace.
pub fn encode_independent_record_batch(
    request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    encode_profile(request, limits, workspace_limit, 6)
}

/// Admits decoded residency before copying a verified nonempty V6 frame.
/// # Errors
/// Rejects wrong profiles, malformed frames, intent or prospective workspace.
pub fn prepare_independent_record_batch(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    prepare_profile(encoded, limits, workspace_limit, 6)
}

/// Inspects exactly one coherent point/cohort gate before any Fresh authority.
/// Counted replay binds incoming item count; whole rejections have no row count.
/// Exact-log, conflict and expiry retain the existing gate precedence.
/// # Errors
/// Rejects profile, intent, workspace, retained grammar/count or gate authority.
pub fn inspect_independent_record_batch_point<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    check_decoded(&decoded, limits, workspace_limit, 6)?;
    let gate = inspect_point_request(view, decoded.header(log), limits.admission)?;
    if let GateDecision::Replay(original) = gate.decision() {
        let (status, count) = response::record_batch_retained_summary(
            original.intent(),
            original.response_version(),
            original.response(),
        )?;
        let PreparedOperation::Batch(items) = &decoded.request.operation else {
            return Err(CodecError::UnsupportedTag.into());
        };
        if status != original.status() || count.is_some_and(|count| count != items.len()) {
            return Err(ApplyError::InvalidAuthority.into());
        }
    }
    Ok(finish_inspection(decoded, log, gate, workspace_limit))
}

/// Consumes Fresh once against reached coherent observations and declared residency.
/// Owns every item/history, borrowing only canonical original predecessors.
/// # Errors
/// Rejects crossed profiles, authority, operational apply, expiry or workspace.
pub fn decide_fresh_independent_record_batch_touched<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    if fresh.decoded.request.intent.version() != 6 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let FreshRecordBatch {
        decoded,
        log,
        control,
        workspace_limit,
    } = fresh;
    let decoded = {
        let owner = decoded;
        *owner
    };
    if view.group != decoded.request.scope.group() {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let operation = super::fresh::decide_independent(
        &decoded,
        view,
        EntryOrder {
            identity: log,
            first_ordinal: 0,
        },
        control.execution_time(),
        workspace_limit,
        additional_live_bytes,
    )?;
    let encoded_len = decoded.encoded.len();
    let limits = decoded.limits;
    let DecodedRequest {
        request,
        isolation,
        state_keys,
        claim_filters,
        ..
    } = decoded;
    drop(request.operation);
    drop(isolation);
    drop(state_keys);
    drop(claim_filters);
    let header = sum(&[
        request.scope.tenant().as_bytes().len(),
        request.scope.group().as_bytes().len(),
        request.scope.recovery().as_bytes().len(),
        request.request.as_bytes().len(),
    ])?;
    let size = response::independent::measure(&operation)?;
    if size > limits.admission.decision_bytes() {
        return Err(CodecError::ByteLimit.into());
    }
    admit(
        sum(&[
            encoded_len,
            limits.admission.decision_bytes(),
            header,
            512,
            size,
            additional_live_bytes,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = response::independent::encode(&operation, size)?;
    let expires_at = control
        .execution_time()
        .checked_add(outcome_ttl_ms)
        .ok_or(RequestValueError::InvalidExpiry)?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: request.scope,
            request: request.request,
            intent: request.intent,
            original_position: log,
            original_time: control.execution_time(),
            expires_at,
            status,
        },
        1,
        response,
        limits.admission,
    )?;
    Ok(RecordBatchApplyPlan {
        control,
        decision: RecordBatchApplyDecision::IndependentFresh {
            operation,
            outcome: Box::new(outcome),
        },
    })
}
