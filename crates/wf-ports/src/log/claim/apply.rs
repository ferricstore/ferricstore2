//! One retained gate and prospectively bounded physical Claim ownership.

use wf_domain::claim::apply::{
    ClaimDecision, ClaimObservedWindow, ClaimRejection, ClaimWorkLimits, decide_observed_claim,
    preflight_observed_claim_query,
};
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::request::gate::{GateDecision, PointGateView, inspect_point_request};
use wf_domain::workflow::request::{RequestValueError, RetainedMetadata, RetainedOutcome};
use wf_types::version::LogIdentity;

use crate::consensus::PreparedOperation;
use crate::log::record_batch::{admit, sum};
use crate::log::{CodecError, CodecLimits, DecodedRequest, RecordBatchApplyError};

use super::super::{common, decode, keys, reader::Reader};
use super::response;

/// An existing retained classification or the same decoded owner after its gate.
pub enum ClaimPhaseInspection<'input, 'retained> {
    /// Current processing control and unchanged original classification.
    Complete(ClaimApplyPlan<'retained>),
    /// Fresh input, with query preflight sealed after the retained inspection.
    Fresh(FreshClaimPhase<'input>),
}

/// Immutable fresh owners; the private preflight is not persistence authority.
pub struct FreshClaimPhase<'input> {
    decoded: Box<DecodedRequest<'input>>,
    log: LogIdentity,
    control: ControlPlan,
    workspace_limit: usize,
    work: ClaimWorkLimits,
    preflight: Result<(usize, u64), ClaimRejection>,
}

impl<'input> FreshClaimPhase<'input> {
    /// Borrows the exact actual query and namespace for current authority reads.
    #[must_use]
    pub fn decoded(&self) -> &DecodedRequest<'input> {
        &self.decoded
    }
    /// Reads processing time/checkpoint independently of original retained data.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
    /// Reads the complete first query walk, including its business rejection.
    /// # Errors
    /// Returns the original sealed query business rejection.
    pub const fn query_preflight(&self) -> Result<(usize, u64), ClaimRejection> {
        self.preflight
    }
}

/// Mathematical control and Claim changes; no durable publication is implied.
#[derive(Debug)]
pub struct ClaimApplyPlan<'current> {
    control: ControlPlan,
    decision: ClaimApplyDecision<'current>,
}

impl<'current> ClaimApplyPlan<'current> {
    /// Reads the proposed processing checkpoint and effective time.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
    /// Borrows the exact fresh operation or original retained classification.
    #[must_use]
    pub const fn decision(&self) -> &ClaimApplyDecision<'current> {
        &self.decision
    }
    /// Moves existing owners after the outer transaction has consumed its guard.
    #[must_use]
    pub fn into_parts(self) -> (ControlPlan, ClaimApplyDecision<'current>) {
        (self.control, self.decision)
    }
}

/// Fresh complete receipt or one unchanged retained classification.
#[derive(Debug)]
pub enum ClaimApplyDecision<'current> {
    /// Persist the touched operation, original receipt and control atomically.
    Fresh {
        /// Accepted touched plans or deterministic business rejection.
        operation: ClaimDecision<'current>,
        /// The single original response owner.
        outcome: Box<RetainedOutcome>,
    },
    /// Borrow the immutable original response before current authority reads.
    Replay(&'current RetainedOutcome),
    /// An existing request ID has a different trusted original intent.
    IntentConflict,
    /// The original retention interval has ended; do not execute again.
    Expired,
    /// Exact full last-log replay changes nothing.
    LogReplay,
}

/// Fully scans op3 and admits all three owned/reference spines before decoding.
/// The bound covers this call's logical owners, not caller residency or native RSS.
/// # Errors
/// Rejects structural/profile errors and prospective workspace before ownership.
pub fn prepare_claim_phase(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit.into());
    }
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    keys::scope_length(header.scope())?;
    if header.operation_tag != 3 {
        return Err(CodecError::UnsupportedTag.into());
    }
    if header.intent.version() != 1 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let count = super::occurrences(&mut reader)?;
    reader.finish()?;
    admit(decoder_bound(encoded.len(), count)?, workspace_limit)?;
    let decoded = decode::read(encoded, limits)?;
    check_decoded(&decoded, limits, workspace_limit)?;
    Ok(decoded)
}

/// Performs exactly one retained gate, then preflights only Fresh query inputs.
/// Generic decoded inputs are already owned; their earlier admission is external.
/// # Errors
/// Rejects inconsistent immutable profiles, retained authority or prospective bounds.
pub fn inspect_claim_phase_point<'input, 'retained>(
    decoded: DecodedRequest<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
    workspace_limit: usize,
    work: ClaimWorkLimits,
) -> Result<ClaimPhaseInspection<'input, 'retained>, RecordBatchApplyError> {
    check_decoded(&decoded, limits, workspace_limit)?;
    let gate = inspect_point_request(view, decoded.header(log), limits.admission())?;
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            let envelope = decoded
                .claim_envelope(log)
                .ok_or(ApplyError::InvalidAuthority)?;
            let preflight = preflight_observed_claim_query(
                &envelope,
                control.execution_time(),
                limits.admission(),
                work,
            )?;
            return Ok(ClaimPhaseInspection::Fresh(FreshClaimPhase {
                decoded: Box::new(decoded),
                log,
                control,
                workspace_limit,
                work,
                preflight,
            }));
        }
        GateDecision::Replay(original) => ClaimApplyDecision::Replay(original),
        GateDecision::IntentConflict => ClaimApplyDecision::IntentConflict,
        GateDecision::Expired => ClaimApplyDecision::Expired,
        GateDecision::LogReplay => ClaimApplyDecision::LogReplay,
    };
    drop(decoded);
    Ok(ClaimPhaseInspection::Complete(ClaimApplyPlan {
        control,
        decision,
    }))
}

/// Consumes Fresh and independently revalidates the query against reached facts.
/// A sealed rejection requires no current window; valid input requires one.
/// The second full query walk adds to the first walk's represented work.
/// # Errors
/// Refuses missing/extraneous observations, authority, allocation and workspace.
pub fn decide_fresh_claim_phase_observed<'current>(
    fresh: FreshClaimPhase<'_>,
    window: Option<ClaimObservedWindow<'_, 'current>>,
    additional_live_bytes: usize,
    outcome_ttl_ms: u64,
) -> Result<ClaimApplyPlan<'current>, RecordBatchApplyError> {
    let FreshClaimPhase {
        decoded,
        log,
        control,
        workspace_limit,
        work,
        preflight,
    } = fresh;
    let decoded = *decoded;
    let base = sum(&[
        check_decoded(&decoded, decoded.limits, workspace_limit)?,
        additional_live_bytes,
    ])?;
    admit(base, workspace_limit)?;
    let operation = match (preflight, window) {
        (Err(reason), None) => ClaimDecision::Rejected(reason),
        (Ok((_, prior_work)), Some(window)) => {
            if window.indexed.group != decoded.request.scope.group() {
                return Err(ApplyError::InvalidAuthority.into());
            }
            admit(
                sum(&[base, decoded.limits.admission().decision_bytes()])?,
                workspace_limit,
            )?;
            let envelope = decoded
                .claim_envelope(log)
                .ok_or(ApplyError::InvalidAuthority)?;
            decide_observed_claim(
                window,
                &envelope,
                control.execution_time(),
                decoded.limits.admission(),
                work,
                prior_work,
            )?
        }
        _ => return Err(ApplyError::InvalidAuthority.into()),
    };
    retain(
        decoded,
        log,
        control,
        operation,
        additional_live_bytes,
        workspace_limit,
        outcome_ttl_ms,
    )
}

fn retain<'current>(
    decoded: DecodedRequest<'_>,
    log: LogIdentity,
    control: ControlPlan,
    operation: ClaimDecision<'current>,
    additional_live_bytes: usize,
    workspace_limit: usize,
    outcome_ttl_ms: u64,
) -> Result<ClaimApplyPlan<'current>, RecordBatchApplyError> {
    let encoded_len = decoded.encoded.len();
    let DecodedRequest {
        request,
        isolation,
        state_keys,
        claim_filters,
        limits,
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
    let output_len = response::measure(&operation, limits.admission())?;
    admit(
        sum(&[
            encoded_len,
            limits.admission().decision_bytes(),
            output_len,
            header_bytes,
            1_024,
            additional_live_bytes,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = response::encode(&operation, limits.admission(), output_len)?;
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
        3,
        response,
        limits.admission(),
    )?;
    Ok(ClaimApplyPlan {
        control,
        decision: ClaimApplyDecision::Fresh {
            operation,
            outcome: Box::new(outcome),
        },
    })
}

fn check_decoded(
    decoded: &DecodedRequest<'_>,
    limits: CodecLimits,
    workspace: usize,
) -> Result<usize, RecordBatchApplyError> {
    if !matches!(decoded.request.operation, PreparedOperation::ClaimPhase(_)) {
        return Err(CodecError::UnsupportedTag.into());
    }
    if decoded.request.intent.version() != 1 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    if decoded.limits != limits {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let filters = decoded
        .claim_filters
        .as_ref()
        .ok_or(ApplyError::InvalidAuthority)?;
    let count = sum(&[filters[0].len(), filters[1].len(), filters[2].len()])?;
    let bound = decoder_bound(decoded.encoded.len(), count)?;
    admit(bound, workspace)?;
    Ok(bound)
}

fn decoder_bound(encoded: usize, count: usize) -> Result<usize, CodecError> {
    sum(&[
        encoded.checked_mul(4).ok_or(CodecError::LengthOverflow)?,
        2_048,
        count.checked_mul(576).ok_or(CodecError::LengthOverflow)?,
    ])
}
