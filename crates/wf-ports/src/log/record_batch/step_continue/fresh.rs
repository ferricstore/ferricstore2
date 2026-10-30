//! Explicit Step retained inspection and consumed proposals, without commit.

use std::mem::size_of;
use wf_domain::workflow::Operation;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{ControlPlan, EntryIdentity};
use wf_domain::workflow::indexed::{EntryOrder, IndexedDecision, TouchedIndexedGroupView};
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
use wf_types::version::LogIdentity;

use super::super::fresh::FreshAuthority;
use super::super::phase::finish_inspection;
use super::super::{
    FreshRecordBatch, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan,
    RecordBatchInspection, admit, check_decoded, effects_apply_error, release_decoded_inputs,
    retain_record_batch_outcome, sum, value_effects_budget,
};
use super::response;
use crate::blob::{RecordBatchValueEffect, RecordBatchValueEffects, build_atomic_value_effects};
use crate::consensus::PreparedOperation;
use crate::log::{CodecError, CodecLimits, DecodedRequest};

/// Inspects one original retained outcome before current Step authority.
/// # Errors
/// Refuses crossed profiles/limits, retained corruption and prospective admission.
pub fn inspect_step_continue_point<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    check_step(&decoded, limits, workspace_limit)?;
    let gate = inspect_point_request(view, decoded.header(log), limits.admission)?;
    if let GateDecision::Replay(original) = gate.decision() {
        let (status, count) = response::step_continue_retained_summary(
            original.intent(),
            original.response_version(),
            original.response(),
            original.original_position(),
            original.original_time(),
            limits.admission,
        )?;
        let expected = match status {
            RetainedStatus::Accepted => Some(1),
            RetainedStatus::IndexedRejected(_) => None,
            _ => return Err(ApplyError::InvalidAuthority.into()),
        };
        if status != original.status() || count != expected {
            return Err(ApplyError::InvalidAuthority.into());
        }
    }
    finish_inspection(decoded, log, gate, workspace_limit)
}

/// Consumes Fresh Step inputs into a proposal and original response, without commit.
/// # Errors
/// Refuses crossed authority, unsupported fields, expiry and resource limits.
pub fn decide_fresh_step_continue_touched<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    check_step(&fresh.decoded, fresh.decoded.limits, fresh.workspace_limit)?;
    let FreshRecordBatch {
        decoded,
        log,
        control,
        workspace_limit,
    } = fresh;
    let decoded = *decoded;
    if view.group != decoded.request.scope.group()
        || control.next().last_entry() != Some(EntryIdentity::new(log, decoded.digest()))
    {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let operation = super::super::fresh::decide(
        &decoded,
        FreshAuthority::Touched(view),
        EntryOrder {
            identity: log,
            first_ordinal: 0,
        },
        control.execution_time(),
        decoded.limits,
        workspace_limit,
        additional_live_bytes,
    )?;
    finish(
        decoded,
        control,
        operation,
        workspace_limit,
        additional_live_bytes,
        outcome_ttl_ms,
    )
}

fn check_step(
    decoded: &DecodedRequest<'_>,
    limits: CodecLimits,
    workspace: usize,
) -> Result<(), RecordBatchApplyError> {
    let version = decoded.request.intent.version();
    if !matches!(version, 10 | 11) {
        return Err(CodecError::UnsupportedVersion.into());
    }
    check_decoded(decoded, limits, workspace, version)?;
    let PreparedOperation::Batch(items) = &decoded.request.operation else {
        return Err(CodecError::UnsupportedTag.into());
    };
    let [item] = items.as_slice() else {
        return Err(CodecError::ItemLimit.into());
    };
    if !matches!(item.command.operation, Operation::StepContinue(_)) {
        return Err(CodecError::UnsupportedTag.into());
    }
    Ok(())
}

fn finish<'current>(
    decoded: DecodedRequest<'_>,
    control: ControlPlan,
    operation: IndexedDecision<'current>,
    workspace: usize,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    let log = control
        .next()
        .last_entry()
        .ok_or(ApplyError::InvalidAuthority)?
        .log();
    let time = control.execution_time();
    let limits = decoded.limits;
    let (live, cap, work) = value_effects_budget(&decoded, workspace, additional_live_bytes)?;
    let value_effects =
        build_atomic_value_effects(&decoded, control, &operation, (live, cap), work)
            .map_err(|error| effects_apply_error(&error))?;
    let effect_bytes = value_effects
        .as_ref()
        .map_or(0, |effects| effects.steps().len())
        .checked_mul(size_of::<RecordBatchValueEffect>())
        .and_then(|rows| rows.checked_add(size_of::<Option<RecordBatchValueEffects>>()))
        .ok_or(CodecError::LengthOverflow)?;
    let encoded_len = decoded.encoded.len();
    let (scope, request, intent) = release_decoded_inputs(decoded);
    let header = sum(&[
        scope.tenant().as_bytes().len(),
        scope.group().as_bytes().len(),
        scope.recovery().as_bytes().len(),
        request.as_bytes().len(),
    ])?;
    let size = response::measure(&operation, log, time, limits.admission)?;
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
        workspace,
    )?;
    let (bytes, status) = response::encode_measured(&operation, log, time, limits.admission, size)?;
    let outcome = retain_record_batch_outcome(
        (scope, request, intent),
        log,
        time,
        outcome_ttl_ms,
        (9, bytes, status),
        limits.admission,
    )?;
    Ok(RecordBatchApplyPlan {
        control,
        value_effects,
        decision: RecordBatchApplyDecision::Fresh { operation, outcome },
    })
}
