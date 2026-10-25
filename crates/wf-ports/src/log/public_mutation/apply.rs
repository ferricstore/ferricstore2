//! One verified public profile gate and the shared independent Fresh planner.

use std::mem::size_of;

use crate::blob::{
    RecordBatchValueEffect, RecordBatchValueEffects, build_independent_value_effects,
};
use crate::log::record_batch::{
    effects_apply_error, release_decoded_inputs, retain_record_batch_outcome, value_effects_budget,
};
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::indexed::{EntryOrder, TouchedIndexedGroupView};
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
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
    finish_inspection(decoded, log, gate, workspace)
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
    let decoded = *decoded;
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
            effect_bytes,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = super::response::encode(&operation, size)?;
    let outcome = retain_record_batch_outcome(
        (scope, request, intent),
        log,
        control.execution_time(),
        outcome_ttl_ms,
        (2, response, status),
        limits.admission,
    )?;
    Ok(RecordBatchApplyPlan {
        control,
        value_effects,
        decision: RecordBatchApplyDecision::IndependentFresh { operation, outcome },
    })
}
