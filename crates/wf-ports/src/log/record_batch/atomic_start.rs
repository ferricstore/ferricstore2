//! Isolated atomic-start preparation, original-response replay and Fresh ownership.

use std::mem::size_of;

use crate::blob::{RecordBatchValueEffect, RecordBatchValueEffects, build_atomic_value_effects};
use crate::log::record_batch::{
    effects_apply_error, release_decoded_inputs, retain_record_batch_outcome, value_effects_budget,
};
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::EntryIdentity;
use wf_domain::workflow::indexed::{EntryOrder, TouchedIndexedGroupView};
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
use wf_types::version::LogIdentity;

use crate::log::{CodecError, CodecLimits, DecodedRequest};

use super::fresh::FreshAuthority;
use super::phase::finish_inspection;
use super::{
    FreshRecordBatch, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan,
    RecordBatchInspection, admit, check_decoded, sum,
};

mod codec;
pub(in crate::log) mod response;

pub use codec::{encode_atomic_start, prepare_atomic_start};
pub use response::{atomic_start_response_matches, atomic_start_retained_summary};

/// Inspects original retained bytes before any current workflow authority.
/// Only Replay exposes a stored response and therefore scans its full grammar.
/// # Errors
/// Preserves generic gate precedence, then original response/cap/status errors.
pub fn inspect_atomic_start_point<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    check_decoded(&decoded, limits, workspace_limit, 7)?;
    let gate = inspect_point_request(view, decoded.header(log), limits.admission)?;
    if !matches!(decoded.request.intent.version(), 8 | 9 | 22)
        && let GateDecision::Replay(original) = gate.decision()
    {
        let (status, count) = atomic_start_retained_summary(
            original.intent(),
            original.response_version(),
            original.response(),
            original.original_position(),
            original.original_time(),
            decoded.limits.admission,
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

/// Consumes actual Fresh authority once, moving the admitted response owner.
/// The late logical bound applies after decoded commands/keys/filter scratch drop.
/// # Errors
/// Rejects profile, authority, expiry, framing or prospective workspace failure.
pub fn decide_fresh_atomic_start_touched<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    if super::super::value_batch::response::effective_profile(&fresh.decoded)? != 7 {
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
    if control.next().last_entry().map(EntryIdentity::log) != Some(log) {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let time = control.execution_time();
    let limits = decoded.limits;
    let response_version = super::super::value_batch::response::response_version(&decoded, 4)?;
    let admission = limits.admission;
    let operation = super::fresh::decide(
        &decoded,
        FreshAuthority::Touched(view),
        EntryOrder {
            identity: log,
            first_ordinal: 0,
        },
        time,
        limits,
        workspace_limit,
        additional_live_bytes,
    )?;
    let (live, cap, work) = value_effects_budget(&decoded, workspace_limit, additional_live_bytes)?;
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
    let size = response::measure(&operation, log, time, admission)?;
    admit(
        sum(&[
            encoded_len,
            admission.decision_bytes(),
            header,
            512,
            size,
            additional_live_bytes,
            effect_bytes,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = response::encode(&operation, log, time, admission, size)?;
    let outcome = retain_record_batch_outcome(
        (scope, request, intent),
        log,
        time,
        outcome_ttl_ms,
        (response_version, response, status),
        admission,
    )?;
    Ok(RecordBatchApplyPlan {
        control,
        value_effects,
        decision: RecordBatchApplyDecision::Fresh { operation, outcome },
    })
}

#[cfg(test)]
mod tests;
