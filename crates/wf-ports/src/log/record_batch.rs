//! One private mutation-batch verification, replay gate and indexed Fresh decision.

use std::collections::BTreeMap;
use std::fmt;

use wf_domain::claim::ScopedRecordId;
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::Operation;
use wf_domain::workflow::batch::{ApplyError, BatchItem};
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::{
    EntryOrder, IndexedBatchItem, IndexedDecision, IndexedGroupView, decide_indexed_batch,
};
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{ClientIntent, RequestValueError, RetainedOutcome};
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::consensus::{PreparedOperation, PreparedRequest};

use super::{
    CodecError, CodecLimits, DecodedRequest, PreparedBytes, common, decode, digest, encode,
    reader::Reader, scan, writer::Writer,
};

mod intent;
mod phase;
mod response;

pub use phase::{
    FreshRecordBatch, RecordBatchInspection, decide_fresh_record_batch, inspect_record_batch,
    inspect_record_batch_point,
};
pub use response::record_batch_retained_status;

/// Proposed control and indexed operation, without durable-apply authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordBatchApplyPlan<'view> {
    control: ControlPlan,
    decision: RecordBatchApplyDecision<'view>,
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
    /// Transfers the operation/outcome without borrowing decoded input scratch.
    #[must_use]
    pub fn into_parts(self) -> (ControlPlan, RecordBatchApplyDecision<'view>) {
        (self.control, self.decision)
    }
}

/// Fresh indexed values or one original scoped replay classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordBatchApplyDecision<'view> {
    /// Persist these bounded plans and original acknowledgement atomically.
    Fresh {
        /// Complete accepted plan or deterministic rejection without partial writes.
        operation: IndexedDecision<'view>,
        /// Immutable private scalar acknowledgement/rejection bytes.
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

type ValueRow<'a> = ([&'a [u8]; 2], usize);

/// Consumes typed private mutation inputs and recomputes their V5 caller intent.
///
/// This does not establish public normalization, authorization or submission.
/// The workspace profile covers algorithm owners, not external/native RSS.
///
/// # Errors
/// Refuses unsupported operations, actual framing limits and prospective bounds.
pub fn encode_record_batch(
    mut request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    let (intent, input_bytes) = intent::derive(&request)?;
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
    scan::preflight(encoded, limits)?;
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    if header.operation_tag != 1 {
        return Err(CodecError::UnsupportedTag.into());
    }
    if header.intent.version() != 5 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let count = common::count(&mut reader)?;
    admit(
        decoder_bound(encoded.len(), count, limits)?,
        workspace_limit,
    )?;
    let decoded = decode::read(encoded, limits)?;
    check_decoded(&decoded, limits, workspace_limit)?;
    Ok(decoded)
}

pub(super) fn decoded_intent(request: &PreparedRequest) -> Result<ClientIntent, CodecError> {
    intent::derive(request).map(|(intent, _)| intent)
}

fn check_decoded(
    decoded: &DecodedRequest<'_>,
    limits: CodecLimits,
    workspace: usize,
) -> Result<(), RecordBatchApplyError> {
    let PreparedOperation::Batch(items) = &decoded.request.operation else {
        return Err(CodecError::UnsupportedTag.into());
    };
    if decoded.request.intent.version() != 5 {
        return Err(CodecError::UnsupportedVersion.into());
    }
    if decoded.limits != limits {
        return Err(ApplyError::InvalidAuthority.into());
    }
    admit(
        decoder_bound(decoded.encoded.len(), items.len(), limits)?,
        workspace,
    )?;
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

fn fresh<'view>(
    decoded: &DecodedRequest<'_>,
    view: IndexedGroupView<'view>,
    retry_policies: &BTreeMap<&[u8], &InstalledRetryPolicy>,
    order: EntryOrder,
    now: TimestampMillis,
    limits: CodecLimits,
    workspace: usize,
) -> Result<IndexedDecision<'view>, RecordBatchApplyError> {
    let PreparedOperation::Batch(prepared) = &decoded.request.operation else {
        return Err(CodecError::UnsupportedTag.into());
    };
    if prepared.iter().any(|item| item.command.now != now) {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let count = prepared.len();
    let bound = sum(&[
        decoder_bound(decoded.encoded.len(), count, limits)?,
        product(count, 192)?,
        256,
        limits.admission.decision_bytes(),
    ])?;
    admit(bound, workspace)?;
    let mut values: Vec<ValueRow<'_>> = reserve(count)?;
    for index in 0..count {
        let mut row: ValueRow<'_> = ([&[]; 2], 0);
        for value in decoded
            .value_bytes(index)
            .ok_or(CodecError::Malformed)?
            .into_iter()
            .flatten()
        {
            row.0[row.1] = value;
            row.1 += 1;
        }
        values.push(row);
    }
    let mut items = reserve(count)?;
    for (index, (prepared, row)) in prepared.iter().zip(&values).enumerate() {
        let policy = if matches!(prepared.command.operation, Operation::RetryWithPolicy(_)) {
            let key = ScopedRecordId {
                partition_key: prepared.partition.as_bytes(),
                id: prepared.command.id.as_bytes(),
            };
            view.batch
                .records
                .get(&key)
                .and_then(|record| retry_policies.get(record.workflow_type()))
                .copied()
        } else {
            None
        };
        items.push(IndexedBatchItem {
            item: BatchItem {
                partition: &prepared.partition,
                command: &prepared.command,
                state_key: decoded.state_key(index).ok_or(CodecError::Malformed)?,
                encoded_values: &row.0[..row.1],
                retry_policy: policy,
            },
            create_priority: prepared.create_priority,
        });
    }
    Ok(decide_indexed_batch(
        view,
        &items,
        order,
        decoded.encoded,
        limits.admission,
    )?)
}

fn decoder_bound(encoded: usize, count: usize, limits: CodecLimits) -> Result<usize, CodecError> {
    sum(&[
        product(encoded, 4)?,
        limits.admission.decision_bytes(),
        product(count, 1_024)?,
        2_048,
    ])
}

fn product(value: usize, multiplier: usize) -> Result<usize, CodecError> {
    value
        .checked_mul(multiplier)
        .ok_or(CodecError::LengthOverflow)
}

fn sum(values: &[usize]) -> Result<usize, CodecError> {
    values.iter().try_fold(0_usize, |sum, value| {
        sum.checked_add(*value).ok_or(CodecError::LengthOverflow)
    })
}

fn admit(attempted: usize, maximum: usize) -> Result<(), RecordBatchApplyError> {
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
