//! One verified public profile gate and the shared independent Fresh planner.

use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::indexed::{EntryOrder, TouchedIndexedGroupView};
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
use wf_domain::workflow::request::{RequestValueError, RetainedMetadata, RetainedOutcome};
use wf_types::version::LogIdentity;

use super::super::{
    CodecError, CodecLimits, DecodedRequest, FreshRecordBatch, RecordBatchApplyDecision,
    RecordBatchApplyError, RecordBatchApplyPlan, RecordBatchInspection, common, decode,
    reader::Reader,
    record_batch::{
        admit, check_decoded, decide_independent, decoder_bound, finish_inspection, sum,
    },
    scan,
};

/// Admits residency before materializing the exact root4/V1 single-item profile.
/// # Errors
/// Rejects malformed/crossed profiles, caller evidence/digest or workspace.
pub fn prepare_public_mutation(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    scan::preflight(encoded, limits)?;
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    if header.operation_tag != 4 || header.intent.version() != 1 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    admit(decoder_bound(encoded.len(), 1, limits)?, workspace_limit)?;
    let decoded = decode::read(encoded, limits)?;
    check_decoded(&decoded, limits, workspace_limit, 1)?;
    Ok(decoded)
}

/// Inspects exactly one coherent retained point before all current authority.
/// # Errors
/// Rejects operational authority/profile or ordinary replay grammar/status errors.
pub fn inspect_public_mutation_point<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
    workspace: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    check_decoded(&decoded, limits, workspace, 1)?;
    let gate = inspect_point_request(view, decoded.header(log), limits.admission)?;
    if let GateDecision::Replay(original) = gate.decision()
        && super::response::public_mutation_retained_status(
            original.intent(),
            original.response_version(),
            original.response(),
        )? != original.status()
    {
        return Err(ApplyError::InvalidAuthority.into());
    }
    Ok(finish_inspection(decoded, log, gate, workspace))
}

/// Consumes public Fresh once, retaining only original canonical borrows.
/// # Errors
/// Rejects crossed profiles, operational authority, budgets or expiry overflow.
pub fn decide_fresh_public_mutation_touched<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    if fresh.decoded.request.intent.version() != 1
        || !matches!(
            fresh.decoded.request.operation,
            crate::consensus::PreparedOperation::PublicMutation { .. }
        )
    {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let FreshRecordBatch {
        decoded,
        log,
        control,
        workspace_limit,
    } = fresh;
    let decoded = {
        let boxed = decoded;
        *boxed
    };
    if view.group != decoded.request.scope.group() {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let operation = decide_independent(
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
    let size = super::response::measure(&operation)?;
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
    let (response, status) = super::response::encode(&operation, size)?;
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
        2,
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
