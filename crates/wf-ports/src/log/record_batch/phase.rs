//! One retained inspection, with current indexed authority deferred until Fresh.

use std::collections::BTreeMap;

use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::{EntryOrder, IndexedGroupView};
use wf_domain::workflow::request::gate::{GateDecision, GateView, inspect_request};
use wf_domain::workflow::request::{RequestValueError, RetainedMetadata, RetainedOutcome};
use wf_types::version::LogIdentity;

use super::super::{CodecLimits, DecodedRequest};
use super::{
    RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan, admit, check_decoded,
    response, sum,
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
    decoded: Box<DecodedRequest<'input>>,
    log: LogIdentity,
    control: ControlPlan,
    workspace_limit: usize,
}

impl<'input> FreshRecordBatch<'input> {
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
    check_decoded(&decoded, limits, workspace_limit)?;
    let gate = inspect_request(view, decoded.header(log), limits.admission)?;
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
    drop(decoded);
    Ok(RecordBatchInspection::Complete(RecordBatchApplyPlan {
        control,
        decision,
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
    let limits = decoded.limits;
    if view.batch.group != decoded.request.scope.group() {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let operation = super::fresh(
        &decoded,
        view,
        retry_policies,
        EntryOrder {
            identity: log,
            first_ordinal: 0,
        },
        control.execution_time(),
        limits,
        workspace_limit,
    )?;
    let encoded_len = decoded.encoded.len();
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
    let header_bytes = sum(&[
        request.scope.tenant().as_bytes().len(),
        request.scope.group().as_bytes().len(),
        request.scope.recovery().as_bytes().len(),
        request.request.as_bytes().len(),
    ])?;
    admit(
        sum(&[
            encoded_len,
            limits.admission.decision_bytes(),
            header_bytes,
            531,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = response::encode(&operation)?;
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
        decision: RecordBatchApplyDecision::Fresh {
            operation,
            outcome: Box::new(outcome),
        },
    })
}
