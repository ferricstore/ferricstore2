//! One FWAS grammar for measurement, exact emission and actual-plan comparison.

use wf_domain::history::EventKind;
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision, OrderingMetadata};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_domain::workflow::{MAX_EXACT_MILLIS, Outcome, WorkflowRecord};
use wf_types::identity::PartitionId;
use wf_types::version::{Fence, LogIdentity, TimestampMillis};

use crate::log::{CodecError, writer::Writer};

mod scan;

/// Scans complete original bytes under the supplied current immutable limits.
/// # Errors
/// Rejects unsupported profiles, malformed values, cap violations and trailing bytes.
pub fn atomic_start_retained_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    original_position: LogIdentity,
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 7 || response_version != 4 {
        return Err(CodecError::UnsupportedVersion);
    }
    scan::summary(response, original_position, original_time, admission)
}

/// Scans syntax first, then verifies the same grammar against the actual decision.
/// Full original term/node authority is the Fresh wrapper's separate obligation.
/// # Errors
/// Preserves syntax/cap/arithmetic failures; valid alternative content returns false.
pub fn atomic_start_response_matches(
    decision: &IndexedDecision<'_>,
    response: &[u8],
    original_position: LogIdentity,
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<bool, CodecError> {
    scan::summary(response, original_position, original_time, admission)?;
    let length = match measure(decision, original_position, original_time, admission) {
        Ok(length) => length,
        Err(CodecError::Malformed) => return Ok(false),
        Err(error) => return Err(error),
    };
    if length != response.len() {
        return Ok(false);
    }
    let mut writer = Writer::verify(response, admission.decision_bytes())?;
    match write(
        decision,
        original_position,
        original_time,
        &mut writer,
        admission,
    )
    .and_then(|_| writer.finish_verification())
    {
        Ok(()) => Ok(true),
        Err(CodecError::Malformed) => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn measure(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<usize, CodecError> {
    let mut writer = Writer::measure(limits.decision_bytes());
    write(decision, position, time, &mut writer, limits)?;
    Ok(writer.len())
}

pub(super) fn encode(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
    length: usize,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    if length > limits.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut writer = Writer::allocate(length, length)?;
    let status = write(decision, position, time, &mut writer, limits)?;
    if writer.len() != length {
        return Err(CodecError::Malformed);
    }
    Ok((writer.finish()?, status))
}

fn write(
    decision: &IndexedDecision<'_>,
    position: LogIdentity,
    time: TimestampMillis,
    writer: &mut Writer<'_>,
    limits: AdmissionLimits,
) -> Result<RetainedStatus, CodecError> {
    writer.write(b"FWAS")?;
    writer.u16(1)?;
    let batch = match decision {
        IndexedDecision::Accepted(batch) => batch,
        IndexedDecision::Rejected(reason) => {
            write_rejection(*reason, writer)?;
            return Ok(RetainedStatus::IndexedRejected(*reason));
        }
    };
    if batch.batch().items().len() != 1
        || batch.metadata().len() != 1
        || batch.consumed_ordinals() != 1
        || batch.next_ordinal() != Some(1)
    {
        return Err(CodecError::Malformed);
    }
    let item = &batch.batch().items()[0];
    let actual = item.plan().write().ok_or(CodecError::Malformed)?;
    let event = item.plan().history().ok_or(CodecError::Malformed)?;
    let lease = actual.record.lease().ok_or(CodecError::Malformed)?;
    if item.plan().outcome() != Outcome::Changed
        || actual.expected_version.is_some()
        || event.kind != EventKind::Created
        || event.occurred_at != time
        || event.version != actual.record.version()
        || event.fence != actual.record.fence()
        || event.attempts != actual.record.attempts()
        || event.state.as_slice() != actual.record.state()
        || event.next_run_at != actual.record.next_run_at()
        || event.lease_deadline != lease.deadline
        || event.worker.as_deref() != Some(lease.worker.as_slice())
        || event.retry.is_some()
    {
        return Err(CodecError::Malformed);
    }
    let digits = event
        .event_id
        .as_bytes()
        .strip_suffix(b"-1")
        .ok_or(CodecError::Malformed)?;
    if digits.is_empty() || digits.len() > 20 || (digits.len() > 1 && digits[0] == b'0') {
        return Err(CodecError::Malformed);
    }
    let event_time = digits.iter().try_fold(0_u64, |value, byte| {
        let digit = byte
            .checked_sub(b'0')
            .filter(|digit| *digit < 10)
            .ok_or(CodecError::Malformed)?;
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(digit)))
            .ok_or(CodecError::LengthOverflow)
    })?;
    if event_time != time.get() {
        return Err(CodecError::Malformed);
    }
    writer.byte(0)?;
    write_record(
        &actual.record,
        item.partition(),
        batch.metadata().next().ok_or(CodecError::Malformed)?,
        position,
        time,
        writer,
        limits,
    )?;
    Ok(RetainedStatus::Accepted)
}

fn write_record(
    record: &WorkflowRecord,
    partition: &PartitionId,
    metadata: OrderingMetadata,
    position: LogIdentity,
    time: TimestampMillis,
    writer: &mut Writer<'_>,
    limits: AdmissionLimits,
) -> Result<(), CodecError> {
    if limits.inline_bytes() < b"running".len() {
        return Err(CodecError::ValueLimit);
    }
    let lease = record.lease().ok_or(CodecError::Malformed)?;
    let logical = record.run_state().ok_or(CodecError::Malformed)?;
    if time.get() > MAX_EXACT_MILLIS
        || record.state() != b"running"
        || record.version() != 1
        || record.fence() != Fence::new(1)
        || record.attempts() != 1
        || record.created_at() != time
        || record.updated_at() != time
        || lease.deadline <= time
        || lease.deadline.get() > MAX_EXACT_MILLIS
        || record.next_run_at() != Some(lease.deadline)
        || record.latest_event_witness() != (EventKind::Created, 1, Fence::new(1))
        || matches!(
            logical,
            b"running" | b"completed" | b"failed" | b"cancelled"
        )
        || record.workflow_type() == b"__ferricstore_schedule"
        || record
            .id()
            .as_bytes()
            .starts_with(b"__ferricstore_schedule__:")
        || partition.as_bytes().is_empty()
        || metadata.priority > 2
        || metadata.state_enter_seq != u128::from(position.index().get()) << 32
        || record.result().is_some()
        || record.error().is_some()
    {
        return Err(CodecError::Malformed);
    }
    writer.raw(partition.as_bytes(), 65_535)?;
    writer.raw(record.id().as_bytes(), 65_535)?;
    for field in [
        record.workflow_type(),
        logical,
        lease.worker.as_slice(),
        lease.token.as_bytes(),
    ] {
        if field.is_empty() {
            return Err(CodecError::Malformed);
        }
        writer.raw(field, limits.inline_bytes())?;
    }
    scan::canonical_token(lease.token.as_bytes(), &lease.worker, time)?;
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

fn write_rejection(reason: IndexRejection, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.byte(1)?;
    match reason {
        IndexRejection::DirectRunningFifoEntry => writer.write(&[7, 0]),
        IndexRejection::InvalidPriority => writer.write(&[4, 0]),
        IndexRejection::FifoPriority => writer.write(&[5, 0]),
        IndexRejection::NotFifoHead => writer.write(&[6, 0]),
        IndexRejection::Batch(reason) => match reason {
            BatchRejection::DuplicateId => writer.write(&[2, 0]),
            BatchRejection::CrossGroup => writer.write(&[3, 0]),
            BatchRejection::Domain(reason) => {
                let (code, detail) = super::super::response::domain(reason);
                writer.write(&[0, code])?;
                if let Some(detail) = detail {
                    writer.byte(detail)?;
                }
                Ok(())
            }
            BatchRejection::Admission(reason) => {
                writer.write(&[
                    1,
                    match reason {
                        AdmissionError::InvalidLimits => 0,
                        AdmissionError::BatchItems => 1,
                        AdmissionError::KeyTooLarge => 2,
                        AdmissionError::ValueTooLarge { .. } => 3,
                        AdmissionError::DecisionBytes => 4,
                        AdmissionError::LengthOverflow => 5,
                        AdmissionError::EncodedValueCount => 6,
                    },
                ])?;
                if let AdmissionError::ValueTooLarge { bytes, maximum } = reason {
                    writer.u64(u64::try_from(bytes).map_err(|_| CodecError::LengthOverflow)?)?;
                    writer.u64(u64::try_from(maximum).map_err(|_| CodecError::LengthOverflow)?)?;
                }
                Ok(())
            }
        },
    }
}
