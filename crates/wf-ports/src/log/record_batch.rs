//! One private mutation-batch verification, replay gate and indexed Fresh decision.

use std::collections::BTreeMap;
use std::fmt;

use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexedDecision};
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{ClientIntent, RequestValueError, RetainedOutcome};
use wf_types::version::LogIdentity;

use crate::blob::{RecordBatchValueEffects, RecordBatchValueEffectsError};
use crate::consensus::{PreparedOperation, PreparedRequest};

use super::{
    CodecError, CodecLimits, DecodedRequest, PreparedBytes, common, decode, digest, encode,
    reader::Reader, scan, writer::Writer,
};

pub(in crate::log) mod atomic_start;
pub(in crate::log) mod fresh;
pub(in crate::log) mod independent;
mod intent;
pub(in crate::log) use intent::operation as hash_operation;
mod phase;
pub(in crate::log) mod response;
mod step_continue;

pub use step_continue::{encode_step_continue, prepare_step_continue};

pub(super) use fresh::{decide_independent, value_effects_budget};
pub(super) use phase::{finish_inspection, release_decoded_inputs, retain_record_batch_outcome};
pub(super) use response::{push_rejection, rejection};

pub use atomic_start::{
    atomic_start_response_matches, atomic_start_retained_summary,
    decide_fresh_atomic_start_touched, encode_atomic_start, inspect_atomic_start_point,
    prepare_atomic_start,
};
pub use independent::{
    decide_fresh_independent_record_batch_touched, encode_independent_record_batch,
    inspect_independent_record_batch_point, prepare_independent_record_batch,
};
pub use phase::{
    FreshRecordBatch, RecordBatchInspection, decide_fresh_record_batch,
    decide_fresh_record_batch_touched, inspect_record_batch, inspect_record_batch_point,
};
pub use response::{
    independent_record_batch_response_matches, record_batch_retained_status,
    record_batch_retained_summary,
};

/// Proposed control and indexed operation, without durable-apply authority.
#[derive(Debug, Eq, PartialEq)]
pub struct RecordBatchApplyPlan<'view> {
    pub(super) control: ControlPlan,
    pub(super) decision: RecordBatchApplyDecision<'view>,
    pub(super) value_effects: Option<RecordBatchValueEffects>,
}

impl<'view> RecordBatchApplyPlan<'view> {
    /// Reads proposed checkpoint/time independently from original replay metadata.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
    /// Borrows a new indexed decision or the unchanged retained result.
    #[must_use]
    pub const fn decision(&self) -> &RecordBatchApplyDecision<'view> {
        &self.decision
    }
    /// Borrows the same attached numeric owner, without publication authority.
    #[must_use]
    pub const fn value_effects(&self) -> Option<&RecordBatchValueEffects> {
        self.value_effects.as_ref()
    }
    /// Transfers the operation/outcome without borrowing decoded input scratch.
    #[must_use]
    pub fn into_parts(self) -> (ControlPlan, RecordBatchApplyDecision<'view>) {
        (self.control, self.decision)
    }
}

/// Fresh indexed values or one original scoped replay classification.
#[derive(Debug, Eq, PartialEq)]
pub enum RecordBatchApplyDecision<'view> {
    /// Persist these bounded plans and original acknowledgement atomically.
    Fresh {
        /// Complete accepted plan or deterministic rejection without partial writes.
        operation: IndexedDecision<'view>,
        /// Immutable private scalar acknowledgement/rejection bytes.
        outcome: Box<RetainedOutcome>,
    },
    /// Persist final records once and every successful independent history.
    IndependentFresh {
        /// Ordered item results or a whole-profile refusal.
        operation: IndependentIndexedDecision<'view>,
        /// Counted V6 or scalar public response, bound to the represented results.
        outcome: Box<RetainedOutcome>,
    },
    /// Borrow the original owner before changed current guards/indexes.
    Replay(&'view RetainedOutcome),
    /// Stable request identity was reused for different caller fields.
    IntentConflict,
    /// Original retention guarantee has expired; never execute again.
    Expired,
    /// Exact full last entry identity; no new operation or outcome.
    LogReplay,
}

/// Operational refusal; never encoded as a retained business outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordBatchApplyError {
    /// Private framing, arithmetic or allocation failed.
    Encoding(CodecError),
    /// Current/log/time authority is inconsistent or host allocation failed.
    Apply(ApplyError),
    /// Original retention metadata cannot be admitted.
    Retained(RequestValueError),
    /// Recomputed caller field digest disagrees with the actual frame.
    IntentMismatch,
    /// A prospective simultaneous logical owner bound exceeds the supplied cap.
    Workspace {
        /// Full mathematical bound at the refused stage.
        attempted: usize,
        /// Explicit caller-supplied algorithm workspace cap.
        maximum: usize,
    },
}

impl fmt::Display for RecordBatchApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoding(error) => fmt::Display::fmt(error, formatter),
            Self::Apply(error) => fmt::Display::fmt(error, formatter),
            Self::Retained(error) => fmt::Display::fmt(error, formatter),
            Self::IntentMismatch => formatter.write_str("record batch caller intent mismatch"),
            Self::Workspace { attempted, maximum } => write!(
                formatter,
                "record batch workspace {attempted} exceeds {maximum}"
            ),
        }
    }
}
impl std::error::Error for RecordBatchApplyError {}
impl From<CodecError> for RecordBatchApplyError {
    fn from(value: CodecError) -> Self {
        Self::Encoding(value)
    }
}
impl From<ApplyError> for RecordBatchApplyError {
    fn from(value: ApplyError) -> Self {
        Self::Apply(value)
    }
}
impl From<RequestValueError> for RecordBatchApplyError {
    fn from(value: RequestValueError) -> Self {
        Self::Retained(value)
    }
}

pub(in crate::log) fn effects_apply_error(
    error: &(RecordBatchValueEffectsError, Option<(usize, usize)>),
) -> RecordBatchApplyError {
    use RecordBatchValueEffectsError::{InvalidChanges, Limit, ReserveFailed, UnsupportedProfile};
    match error {
        (Limit, Some((attempted, maximum))) => RecordBatchApplyError::Workspace {
            attempted: *attempted,
            maximum: *maximum,
        },
        (InvalidChanges, _) => ApplyError::InvalidAuthority.into(),
        (UnsupportedProfile, _) => CodecError::UnsupportedTag.into(),
        (Limit, None) => CodecError::ByteLimit.into(),
        (ReserveFailed(_), _) => CodecError::Allocation.into(),
    }
}

/// Consumes typed private mutation inputs and recomputes their V5 caller intent.
///
/// This does not establish public normalization, authorization or submission.
/// The workspace profile covers algorithm owners, not external/native RSS.
///
/// # Errors
/// Refuses unsupported operations, actual framing limits and prospective bounds.
pub fn encode_record_batch(
    request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    encode_profile(request, limits, workspace_limit, 5)
}

fn encode_profile(
    mut request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
    version: u16,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    let (intent, input_bytes) = intent::derive(&request, version)?;
    request.intent = intent;
    let PreparedOperation::Batch(items) = &request.operation else {
        return Err(CodecError::UnsupportedTag.into());
    };
    let mut measure = Writer::measure(limits.admission.decision_bytes());
    encode::write(&request, &mut measure, limits)?;
    let bound = sum(&[
        input_bytes,
        product(items.len(), 1_024)?,
        512,
        measure.len(),
    ])?;
    admit(bound, workspace_limit)?;
    let mut output = Writer::allocate(measure.len(), limits.admission.decision_bytes())?;
    encode::write(&request, &mut output, limits)?;
    let bytes = output.finish()?;
    Ok(PreparedBytes {
        digest: digest(&bytes),
        bytes,
    })
}

/// Prospectively bounds decoded owners before preparing one private mutation batch.
///
/// The result provides actual scope/request lookup operands before inspection.
/// This per-call bound does not include previously returned owners held by callers.
///
/// # Errors
/// Rejects framing, unsupported roots/profiles, intent or prospective workspace.
pub fn prepare_record_batch(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    prepare_profile(encoded, limits, workspace_limit, 5)
}

fn prepare_profile(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
    version: u16,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    scan::preflight(encoded, limits)?;
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    if header.operation_tag != 1 {
        return Err(CodecError::UnsupportedTag.into());
    }
    if header.intent.version() != version {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let count = common::count(&mut reader)?;
    admit_decoded_request(encoded.len(), count, limits, workspace_limit)?;
    let decoded = decode::read(encoded, limits)?;
    check_decoded(&decoded, limits, workspace_limit, version)?;
    Ok(decoded)
}

pub(super) fn decoded_intent(request: &PreparedRequest) -> Result<ClientIntent, CodecError> {
    intent::derive(request, request.intent.version()).map(|(intent, _)| intent)
}

pub(super) fn check_decoded(
    decoded: &DecodedRequest<'_>,
    limits: CodecLimits,
    workspace: usize,
    version: u16,
) -> Result<(), RecordBatchApplyError> {
    let items = decoded
        .request
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    let profile = super::value_batch::response::effective_profile(decoded)?;
    if (profile == 1)
        != matches!(
            decoded.request.operation,
            PreparedOperation::PublicMutation { .. }
        )
    {
        return Err(CodecError::UnsupportedVersion.into());
    }
    if profile != version {
        return Err(CodecError::UnsupportedVersion.into());
    }
    if decoded.limits != limits {
        return Err(ApplyError::InvalidAuthority.into());
    }
    admit_decoded_request(decoded.encoded.len(), items.len(), limits, workspace)?;
    if decoded.record_batch_intent? != decoded.request.intent {
        return Err(RecordBatchApplyError::IntentMismatch);
    }
    Ok(())
}

/// Verifies actual private bytes and invokes exactly one scoped replay gate.
///
/// Only Fresh consults current records/retry/index authority. The returned plan
/// borrows current predecessors, independently of decoded command/reference Vecs.
/// Persistence of control, record/history/index changes and outcome is external.
///
/// # Errors
/// Rejects corruption, namespace/time/authority failures and prospective resources.
pub fn decide_record_batch<'view>(
    encoded: &[u8],
    log: LogIdentity,
    view: IndexedRequestView<'view>,
    retry_policies: &BTreeMap<&[u8], &InstalledRetryPolicy>,
    outcome_ttl_ms: u64,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<RecordBatchApplyPlan<'view>, RecordBatchApplyError> {
    let decoded = prepare_record_batch(encoded, limits, workspace_limit)?;
    if view.group.batch.group != view.gate.scope.group() {
        return Err(ApplyError::InvalidAuthority.into());
    }
    match inspect_record_batch(decoded, log, view.gate, limits, workspace_limit)? {
        RecordBatchInspection::Complete(plan) => Ok(plan),
        RecordBatchInspection::Fresh(fresh) => {
            decide_fresh_record_batch(fresh, view.group, retry_policies, outcome_ttl_ms)
        }
    }
}

// Preserve the old refusal first, then cover the complete enlarged header.
// This additional full-header allowance is conservative, not another allocation.
pub(super) fn admit_decoded_request(
    encoded: usize,
    count: usize,
    limits: CodecLimits,
    workspace: usize,
) -> Result<(), RecordBatchApplyError> {
    let bound = decoder_bound(encoded, count, limits)?;
    admit(bound, workspace)?;
    admit(
        sum(&[bound, std::mem::size_of::<DecodedRequest<'static>>()])?,
        workspace,
    )
}

pub(super) fn decoder_bound(
    encoded: usize,
    count: usize,
    limits: CodecLimits,
) -> Result<usize, CodecError> {
    sum(&[
        product(encoded, 4)?,
        limits.admission.decision_bytes(),
        product(count, 1_024)?,
        2_048,
    ])
}

pub(in crate::log) fn product(value: usize, multiplier: usize) -> Result<usize, CodecError> {
    value
        .checked_mul(multiplier)
        .ok_or(CodecError::LengthOverflow)
}

pub(super) fn sum(values: &[usize]) -> Result<usize, CodecError> {
    values.iter().try_fold(0_usize, |sum, value| {
        sum.checked_add(*value).ok_or(CodecError::LengthOverflow)
    })
}

pub(super) fn admit(attempted: usize, maximum: usize) -> Result<(), RecordBatchApplyError> {
    if attempted > maximum {
        Err(RecordBatchApplyError::Workspace { attempted, maximum })
    } else {
        Ok(())
    }
}

fn reserve<T>(count: usize) -> Result<Vec<T>, CodecError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    Ok(values)
}
