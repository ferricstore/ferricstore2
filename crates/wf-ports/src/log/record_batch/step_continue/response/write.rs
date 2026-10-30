//! One measure/emit/verify grammar over an actual immutable Step decision.

use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::{IndexedDecision, OrderingMetadata};
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::{MAX_EXACT_MILLIS, Outcome, WorkflowRecord};
use wf_types::identity::PartitionId;
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::log::{CodecError, writer::Writer};

pub(super) fn write(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    writer: &mut Writer<'_>,
    limits: AdmissionLimits,
) -> Result<RetainedStatus, CodecError> {
    if time.get() > MAX_EXACT_MILLIS {
        return Err(CodecError::Malformed);
    }
    writer.write(b"FWST")?;
    writer.u16(1)?;
    for scalar in [
        position.term(),
        position.leader_node(),
        position.index().get(),
        time.get(),
    ] {
        writer.u64(scalar)?;
    }
    let batch = match decision {
        IndexedDecision::Accepted(batch) => batch,
        IndexedDecision::Rejected(reason) => {
            crate::log::record_batch::atomic_start::response::write_rejection(*reason, writer)?;
            return Ok(RetainedStatus::IndexedRejected(*reason));
        }
    };
    if limits.inline_bytes() < b"running".len() {
        return Err(CodecError::ValueLimit);
    }
    if batch.batch().items().len() != 1
        || batch.metadata().len() != 1
        || batch.consumed_ordinals() > 1
    {
        return Err(CodecError::Malformed);
    }
    let item = &batch.batch().items()[0];
    let actual = item.plan().write().ok_or(CodecError::Malformed)?;
    let event = item.plan().history().ok_or(CodecError::Malformed)?;
    let metadata = batch.metadata().next().ok_or(CodecError::Malformed)?;
    let maximum_sequence = (u128::from(position.index().get()) << 32) | u128::from(u32::MAX);
    if item.plan().outcome() != Outcome::Changed
        || metadata.priority > 2
        || metadata.state_enter_seq > maximum_sequence
    {
        return Err(CodecError::Malformed);
    }
    writer.byte(0)?;
    fields(&actual.record, item.partition(), metadata, writer, limits)?;
    validate(&actual.record, actual.expected_version, event, time)?;
    Ok(RetainedStatus::Accepted)
}

fn fields(
    record: &WorkflowRecord,
    partition: &PartitionId,
    metadata: OrderingMetadata,
    writer: &mut Writer<'_>,
    limits: AdmissionLimits,
) -> Result<(), CodecError> {
    let lease = record.lease().ok_or(CodecError::Malformed)?;
    let logical = record.run_state().ok_or(CodecError::Malformed)?;
    writer.raw(partition.as_bytes(), 65_535)?;
    writer.raw(record.id().as_bytes(), 65_535)?;
    for field in [
        record.workflow_type(),
        logical,
        lease.worker.as_slice(),
        lease.token.as_bytes(),
    ] {
        writer.raw(field, limits.inline_bytes())?;
    }
    for scalar in [
        record.version(),
        record.fence().get(),
        lease.deadline.get(),
        record.created_at().get(),
        record.updated_at().get(),
        record.attempts(),
    ] {
        writer.u64(scalar)?;
    }
    writer.byte(metadata.priority)?;
    writer.write(&metadata.state_enter_seq.to_be_bytes())?;
    for value in [record.payload(), record.result(), record.error()] {
        writer.optional(value, |writer, bytes| {
            writer.raw(bytes, limits.inline_bytes())
        })?;
    }
    Ok(())
}

fn validate(
    record: &WorkflowRecord,
    expected: Option<u64>,
    event: &HistoryEntry,
    time: TimestampMillis,
) -> Result<(), CodecError> {
    let lease = record.lease().ok_or(CodecError::Malformed)?;
    let logical = record.run_state().ok_or(CodecError::Malformed)?;
    if expected
        .is_none_or(|previous| previous == 0 || previous.checked_add(1) != Some(record.version()))
        || record.fence().get() == 0
        || record.state() != b"running"
        || record.workflow_type().is_empty()
        || logical.is_empty()
        || lease.worker.is_empty()
        || matches!(
            logical,
            b"running" | b"completed" | b"failed" | b"cancelled"
        )
        || record.created_at().get() > MAX_EXACT_MILLIS
        || record.updated_at() != time
        || lease.deadline <= time
        || lease.deadline.get() > MAX_EXACT_MILLIS
        || record.next_run_at() != Some(lease.deadline)
        || record.latest_event_witness()
            != (EventKind::StepContinued, record.version(), record.fence())
        || event.kind != EventKind::StepContinued
        || event.occurred_at != time
        || event.version != record.version()
        || event.fence != record.fence()
        || event.attempts != record.attempts()
        || event.state.as_slice() != record.state()
        || event.next_run_at != record.next_run_at()
        || event.lease_deadline != lease.deadline
        || event.worker.as_deref() != Some(lease.worker.as_slice())
        || event.retry.is_some()
    {
        return Err(CodecError::Malformed);
    }
    canonical_token(
        lease.token.as_bytes(),
        &lease.worker,
        time,
        record.fence().get(),
    )
}

pub(super) fn canonical_token(
    token: &[u8],
    worker: &[u8],
    time: TimestampMillis,
    fence: u64,
) -> Result<(), CodecError> {
    let mut writer = Writer::verify(token, usize::MAX)?;
    writer.write(worker)?;
    writer.byte(b':')?;
    decimal(&mut writer, time.get())?;
    writer.byte(b':')?;
    decimal(&mut writer, fence)?;
    writer.finish_verification()
}

fn decimal(writer: &mut Writer<'_>, mut value: u64) -> Result<(), CodecError> {
    let mut digits = [0_u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + u8::try_from(value % 10).map_err(|_| CodecError::Malformed)?;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    writer.write(&digits[start..])
}
