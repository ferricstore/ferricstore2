//! One retained inspection, with current indexed authority deferred until Fresh.

use std::collections::BTreeMap;
use std::mem::size_of;

use crate::blob::{RecordBatchValueEffect, RecordBatchValueEffects, build_atomic_value_effects};
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::{EntryOrder, IndexedGroupView, TouchedIndexedGroupView};
use wf_domain::workflow::request::gate::{
    GateDecision, GateView, PointGateView, RequestGate, inspect_point_request, inspect_request,
};
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RequestValueError, RetainedMetadata, RetainedOutcome,
    RetainedStatus,
};
use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

use super::super::{CodecError, CodecLimits, DecodedRequest};
use super::fresh::FreshAuthority;
use super::{
    RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan, admit, check_decoded,
    decoder_bound, effects_apply_error, response, sum, value_effects_budget,
};

/// Complete retained classification or an opaque fresh decision input.
pub enum RecordBatchInspection<'input, 'retained> {
    /// Existing control and original outcome; no current indexed view is needed.
    Complete(RecordBatchApplyPlan<'retained>),
    /// The same decoded owner after one retained inspection.
    Fresh(FreshRecordBatch<'input>),
}

/// Immutable fresh inputs, without a retained-map borrow or persistence authority.
///
/// The fixed box moves existing buffers; it does not copy commands or keys.
/// Its allocation follows the existing fixed-owner allocator behavior.
pub struct FreshRecordBatch<'input> {
    pub(in crate::log) decoded: Box<DecodedRequest<'input>>,
    pub(in crate::log) log: LogIdentity,
    pub(in crate::log) control: ControlPlan,
    pub(in crate::log) workspace_limit: usize,
}

impl<'input> FreshRecordBatch<'input> {
    /// Moves original metadata operands without cloning their byte allocations.
    pub fn take_record_metadata_updates(&mut self) -> Option<Vec<Option<Vec<u8>>>> {
        self.decoded.take_record_metadata_updates()
    }
    /// Borrows actual scope, request and commands for current authority lookup.
    #[must_use]
    pub fn decoded(&self) -> &DecodedRequest<'input> {
        &self.decoded
    }

    /// Reads current processing control independently from original replay data.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
}

/// Checks actual decoded data/profile and performs exactly one retained gate.
///
/// Generic decoded inputs are already caller-owned; this cannot certify their
/// earlier workspace admission. The view remains coherent retained authority.
/// Current records, retry policies and correctness indexes are not arguments.
///
/// # Errors
/// Rejects unsupported roots/profiles, intent, workspace or retained authority.
pub fn inspect_record_batch<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: GateView<'retained>,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    let profile = if matches!(decoded.request.intent.version(), 8 | 9 | 22) {
        super::super::value_batch::response::effective_profile(&decoded)?
    } else {
        5
    };
    check_decoded(&decoded, limits, workspace_limit, profile)?;
    let gate = inspect_request(view, decoded.header(log), limits.admission)?;
    finish_inspection(decoded, log, gate, workspace_limit)
}

/// Checks actual decoded data and inspects one original outcome and scoped cohort.
///
/// The observations must share one coherent transaction revision. A complete
/// retained map and current indexed authority are not arguments. Generic decoded
/// inputs remain caller-owned before this method's prospective checks.
///
/// # Errors
/// Rejects unsupported roots/profiles, intent, workspace or retained authority.
pub fn inspect_record_batch_point<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    let profile = if matches!(decoded.request.intent.version(), 8 | 9 | 22) {
        super::super::value_batch::response::effective_profile(&decoded)?
    } else {
        5
    };
    check_decoded(&decoded, limits, workspace_limit, profile)?;
    let gate = inspect_point_request(view, decoded.header(log), limits.admission)?;
    finish_inspection(decoded, log, gate, workspace_limit)
}

pub(in crate::log) fn finish_inspection<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    gate: RequestGate<'retained>,
    workspace_limit: usize,
) -> Result<RecordBatchInspection<'input, 'retained>, RecordBatchApplyError> {
    if matches!(decoded.request.intent.version(), 8 | 9 | 22)
        && let GateDecision::Replay(original) = gate.decision()
    {
        super::super::value_batch::response::check_replay(&decoded, original)?;
    }
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            return Ok(RecordBatchInspection::Fresh(FreshRecordBatch {
                decoded: Box::new(decoded),
                log,
                control,
                workspace_limit,
            }));
        }
        GateDecision::Replay(original) => RecordBatchApplyDecision::Replay(original),
        GateDecision::IntentConflict => RecordBatchApplyDecision::IntentConflict,
        GateDecision::Expired => RecordBatchApplyDecision::Expired,
        GateDecision::LogReplay => RecordBatchApplyDecision::LogReplay,
    };
    let count = decoded
        .request
        .operation
        .mutation_items()
        .ok_or(super::super::CodecError::UnsupportedTag)?
        .len();
    let bound = sum(&[
        decoder_bound(decoded.encoded.len(), count, decoded.limits)?,
        size_of::<Option<RecordBatchValueEffects>>(),
    ])?;
    admit(bound, workspace_limit)?;
    admit(
        sum(&[bound, size_of::<DecodedRequest<'static>>()])?,
        workspace_limit,
    )?;
    drop(decoded);
    Ok(RecordBatchInspection::Complete(RecordBatchApplyPlan {
        control,
        decision,
        value_effects: None,
    }))
}

/// Consumes fresh inputs with complete current indexed authority, without gating again.
///
/// The same decoded buffers move once and are released before the late response
/// admission. The returned plan borrows only current predecessors. A partial
/// lane/head window is not the complete indexed view required here.
///
/// # Errors
/// Rejects current authority, time, indexed apply, retention or workspace failures.
pub fn decide_fresh_record_batch<'current>(
    fresh: FreshRecordBatch<'_>,
    view: IndexedGroupView<'current>,
    retry_policies: &BTreeMap<&[u8], &InstalledRetryPolicy>,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    finish_fresh(
        fresh,
        FreshAuthority::Full {
            view,
            retry_policies,
        },
        0,
        outcome_ttl_ms,
    )
}

/// Consumes Fresh with explicit reached observations and declared additional residency.
///
/// Missing observations refuse instead of supplying defaults. Query descriptors
/// and policies need not outlive the returned current-predecessor plan. The
/// caller must separately admit fact-map construction and loaded owners; this
/// scalar adds their declared logical residency, not native allocation proof.
///
/// # Errors
/// Rejects unobserved/corrupt authority, time, apply, retention or workspace failures.
pub fn decide_fresh_record_batch_touched<'current>(
    fresh: FreshRecordBatch<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    finish_fresh(
        fresh,
        FreshAuthority::Touched(view),
        additional_live_bytes,
        outcome_ttl_ms,
    )
}

fn finish_fresh<'current>(
    fresh: FreshRecordBatch<'_>,
    authority: FreshAuthority<'_, 'current>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    if super::super::value_batch::response::effective_profile(&fresh.decoded)? != 5 {
        return Err(super::super::CodecError::UnsupportedVersion.into());
    }
    let FreshRecordBatch {
        decoded,
        log,
        control,
        workspace_limit,
    } = fresh;
    let decoded = *decoded;
    let limits = decoded.limits;
    let response_version = super::super::value_batch::response::response_version(&decoded, 1)?;
    if authority.group() != decoded.request.scope.group() {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let operation = super::fresh::decide(
        &decoded,
        authority,
        EntryOrder {
            identity: log,
            first_ordinal: 0,
        },
        control.execution_time(),
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
    let header_bytes = sum(&[
        scope.tenant().as_bytes().len(),
        scope.group().as_bytes().len(),
        scope.recovery().as_bytes().len(),
        request.as_bytes().len(),
    ])?;
    let late = sum(&[
        encoded_len,
        limits.admission.decision_bytes(),
        header_bytes,
        531,
    ])?;
    admit(
        sum(&[late, additional_live_bytes, effect_bytes])?,
        workspace_limit,
    )?;
    let (response, status) = response::encode(&operation)?;
    let outcome = retain_record_batch_outcome(
        (scope, request, intent),
        log,
        control.execution_time(),
        outcome_ttl_ms,
        (response_version, response, status),
        limits.admission,
    )?;
    Ok(RecordBatchApplyPlan {
        control,
        value_effects,
        decision: RecordBatchApplyDecision::Fresh { operation, outcome },
    })
}

pub(in crate::log) fn release_decoded_inputs(
    decoded: DecodedRequest<'_>,
) -> (RequestScope, RequestId, ClientIntent) {
    let DecodedRequest {
        request,
        isolation,
        state_keys,
        claim_filters,
        value_metadata,
        ..
    } = decoded;
    drop(request.operation);
    drop(isolation);
    drop(state_keys);
    drop(claim_filters);
    drop(value_metadata);
    (request.scope, request.request, request.intent)
}

pub(in crate::log) fn retain_record_batch_outcome(
    header: (RequestScope, RequestId, ClientIntent),
    log: LogIdentity,
    time: TimestampMillis,
    outcome_ttl_ms: u64,
    response: (u16, Vec<u8>, RetainedStatus),
    admission: AdmissionLimits,
) -> Result<Box<RetainedOutcome>, RecordBatchApplyError> {
    let (scope, request, intent) = header;
    let (version, bytes, status) = response;
    let expires_at = time
        .checked_add(outcome_ttl_ms)
        .ok_or(RequestValueError::InvalidExpiry)?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request,
            intent,
            original_position: log,
            original_time: time,
            expires_at,
            status,
        },
        version,
        bytes,
        admission,
    )?;
    Ok(Box::new(outcome))
}
