//! Verified V6 preparation, one point gate and an owned counted Fresh response.

use std::mem::size_of;

use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::indexed::{
    EntryOrder, IndependentIndexedWithOrdering, TouchedIndexedGroupView,
};
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::blob::{
    RecordBatchValueEffect, RecordBatchValueEffects, build_independent_value_effects,
};
use crate::consensus::{PreparedOperation, PreparedRequest};
use crate::log::record_batch::{
    effects_apply_error, release_decoded_inputs, retain_record_batch_outcome, value_effects_budget,
};
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
    if !matches!(decoded.request.intent.version(), 8 | 9)
        && let GateDecision::Replay(original) = gate.decision()
    {
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
    finish_inspection(decoded, log, gate, workspace_limit)
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
    finish_fresh(fresh, view, additional_live_bytes, outcome_ttl_ms, false).map(|(plan, _)| plan)
}

pub(in crate::log) fn decide_with_ordering<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<crate::log::WorkflowValueDecision<'current>, RecordBatchApplyError> {
    if !matches!(fresh.decoded.request.intent.version(), 8 | 9)
        || super::super::value_batch::response::effective_profile(&fresh.decoded)? != 6
    {
        return Err(CodecError::UnsupportedVersion.into());
    }
    finish_fresh(fresh, view, additional_live_bytes, outcome_ttl_ms, true)
}

fn finish_fresh<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
    retain_ordering: bool,
) -> Result<crate::log::WorkflowValueDecision<'current>, RecordBatchApplyError> {
    let FreshRecordBatch {
        decoded,
        log,
        control,
        workspace_limit,
    } = fresh;
    let decoded = *decoded;
    let response_version = super::super::value_batch::response::response_version(&decoded, 1)?;
    if view.group != decoded.request.scope.group() {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let order = EntryOrder {
        identity: log,
        first_ordinal: 0,
    };
    let (operation, ordering) = decide_operation(
        &decoded,
        view,
        order,
        control.execution_time(),
        workspace_limit,
        additional_live_bytes,
        retain_ordering,
    )?;
    let (live, cap, work) = value_effects_budget(&decoded, workspace_limit, additional_live_bytes)?;
    let value_effects =
        build_independent_value_effects(&decoded, control, &operation, (live, cap), work)
            .map_err(|error| effects_apply_error(&error))?;
    let effect_bytes = value_effects
        .as_ref()
        .map_or(0, |effects| effects.steps().len())
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .and_then(|rows| rows.checked_add(size_of::<Option<RecordBatchValueEffects>>()))
        .ok_or(CodecError::LengthOverflow)?;
    let encoded_len = decoded.encoded.len();
    let limits = decoded.limits;
    let (scope, request, intent) = release_decoded_inputs(decoded);
    let header = sum(&[
        scope.tenant().as_bytes().len(),
        scope.group().as_bytes().len(),
        scope.recovery().as_bytes().len(),
        request.as_bytes().len(),
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
            effect_bytes,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = response::independent::encode(&operation, size)?;
    let outcome = retain_record_batch_outcome(
        (scope, request, intent),
        log,
        control.execution_time(),
        outcome_ttl_ms,
        (response_version, response, status),
        limits.admission,
    )?;
    Ok((
        RecordBatchApplyPlan {
            control,
            value_effects,
            decision: RecordBatchApplyDecision::IndependentFresh { operation, outcome },
        },
        ordering,
    ))
}

fn decide_operation<'current>(
    decoded: &DecodedRequest<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    order: EntryOrder,
    time: TimestampMillis,
    workspace_limit: usize,
    additional_live_bytes: usize,
    retain_ordering: bool,
) -> Result<IndependentIndexedWithOrdering<'current>, RecordBatchApplyError> {
    if retain_ordering {
        return super::fresh::decide_independent_with_ordering(
            decoded,
            view,
            order,
            time,
            workspace_limit,
            additional_live_bytes,
        );
    }
    let operation = super::fresh::decide_independent(
        decoded,
        view,
        order,
        time,
        workspace_limit,
        additional_live_bytes,
    )?;
    Ok((operation, Vec::new()))
}
