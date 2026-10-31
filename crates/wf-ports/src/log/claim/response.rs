//! One FWCR grammar for measurement, exact emission and plan correspondence.

use wf_domain::claim::SelectionStop;
use wf_domain::claim::apply::{ClaimDecision, ClaimRejection};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_domain::workflow::{Outcome, WorkflowRecord};
use wf_types::version::TimestampMillis;

use crate::log::{CodecError, reader::Reader, writer::Writer};

pub(in crate::log) mod scan;

/// Fully validates a Claim receipt against its original stored metadata/profile.
/// A narrower-profile refusal establishes no health proof for unread content.
/// # Errors
/// Rejects unsupported profiles, malformed hard grammar and configured byte/count caps.
pub fn claim_retained_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 1 || response_version != 3 {
        return Err(CodecError::UnsupportedVersion);
    }
    scan::summary(response, original_time, admission)
}

/// Compares every ordered postimage, optional presence and selection frontier.
/// The body is fully scanned before the allocation-free expected-plan comparison.
/// # Errors
/// Rejects invalid body/time/profile or malformed mathematical plan grammar.
pub fn claim_response_matches(
    decision: &ClaimDecision<'_>,
    response: &[u8],
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<bool, CodecError> {
    scan::summary(response, original_time, admission)?;
    let length = measure(decision, admission)?;
    if length != response.len() {
        return Ok(false);
    }
    let mut writer = Writer::verify(response, admission.decision_bytes())?;
    match write(decision, &mut writer, admission).and_then(|_| writer.finish_verification()) {
        Ok(()) => Ok(true),
        Err(CodecError::Malformed) => Ok(false),
        Err(error) => Err(error),
    }
}

pub(in crate::log) fn measure(
    decision: &ClaimDecision<'_>,
    admission: AdmissionLimits,
) -> Result<usize, CodecError> {
    let mut writer = Writer::measure(admission.decision_bytes());
    write(decision, &mut writer, admission)?;
    Ok(writer.len())
}

pub(super) fn encode(
    decision: &ClaimDecision<'_>,
    admission: AdmissionLimits,
    length: usize,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    if length > admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut writer = Writer::allocate(length, length)?;
    let status = write(decision, &mut writer, admission)?;
    if writer.len() != length {
        return Err(CodecError::Malformed);
    }
    Ok((writer.finish()?, status))
}

pub(in crate::log) fn write(
    decision: &ClaimDecision<'_>,
    writer: &mut Writer<'_>,
    admission: AdmissionLimits,
) -> Result<RetainedStatus, CodecError> {
    writer.write(b"FWCR")?;
    writer.u16(1)?;
    let ClaimDecision::Accepted(plan) = decision else {
        let ClaimDecision::Rejected(reason) = decision else {
            return Err(CodecError::Malformed);
        };
        writer.byte(1)?;
        write_rejection(*reason, writer)?;
        return Ok(RetainedStatus::ClaimRejected(*reason));
    };
    let count = plan.batch().map_or(0, |batch| batch.batch().items().len());
    if count > 1_000
        || plan.scanned() > 16_384
        || plan.head_reads() > 4_096
        || count > plan.scanned()
    {
        return Err(CodecError::Malformed);
    }
    if count > admission.batch_items() {
        return Err(CodecError::ItemLimit);
    }
    let stop = match plan.stop() {
        SelectionStop::Complete => 0,
        SelectionStop::ClaimLimit => 1,
        SelectionStop::ScanBudget => 2,
    };
    writer.byte(0)?;
    writer.byte(stop)?;
    writer.byte(u8::from(stop != 0))?;
    for value in [plan.scanned(), plan.head_reads(), count] {
        writer.u32(u32::try_from(value).map_err(|_| CodecError::LengthOverflow)?)?;
    }
    if let Some(batch) = plan.batch() {
        if batch.metadata().len() != count {
            return Err(CodecError::Malformed);
        }
        for (item, metadata) in batch.batch().items().iter().zip(batch.metadata()) {
            if item.plan().outcome() != Outcome::Changed || item.plan().history().is_none() {
                return Err(CodecError::Malformed);
            }
            let write = item.plan().write().ok_or(CodecError::Malformed)?;
            write_record(
                &write.record,
                item.partition().as_bytes(),
                metadata,
                writer,
                admission,
            )?;
        }
    }
    Ok(RetainedStatus::Accepted)
}

fn write_record(
    record: &WorkflowRecord,
    partition: &[u8],
    metadata: OrderingMetadata,
    writer: &mut Writer<'_>,
    admission: AdmissionLimits,
) -> Result<(), CodecError> {
    let lease = record.lease().ok_or(CodecError::Malformed)?;
    let run_state = record.run_state().ok_or(CodecError::Malformed)?;
    if record.state() != b"running"
        || record.next_run_at() != Some(lease.deadline)
        || partition.is_empty()
        || record.version() == 0
        || record.fence().get() == 0
        || metadata.priority > 2
    {
        return Err(CodecError::Malformed);
    }
    writer.raw(partition, 65_535)?;
    writer.raw(record.id().as_bytes(), 65_535)?;
    for field in [
        record.workflow_type(),
        run_state,
        &lease.worker,
        lease.token.as_bytes(),
    ] {
        if field.is_empty() {
            return Err(CodecError::Malformed);
        }
        writer.raw(field, admission.inline_bytes())?;
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
            writer.raw(bytes, admission.inline_bytes())
        })?;
    }
    Ok(())
}

fn write_rejection(reason: ClaimRejection, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    let tag = match reason {
        ClaimRejection::GoverningReservationUnavailable => {
            return Err(CodecError::UnsupportedVersion);
        }
        ClaimRejection::InvalidScope => 0,
        ClaimRejection::InvalidPriority => 1,
        ClaimRejection::InvalidBudget => 2,
        ClaimRejection::HeadBudget => 3,
        ClaimRejection::StaleContinuation => 4,
        ClaimRejection::EmptyType => 5,
        ClaimRejection::EmptyWorker => 6,
        ClaimRejection::ZeroLease => 7,
        ClaimRejection::LeaseTooLarge => 8,
        ClaimRejection::LeaseDeadlineTooLarge => 9,
        ClaimRejection::VersionOverflow => 10,
        ClaimRejection::FenceOverflow => 11,
        ClaimRejection::ValueTooLarge { .. } => 12,
        ClaimRejection::DecisionBytes => 13,
        ClaimRejection::LengthOverflow => 14,
        ClaimRejection::WorkBytes { .. } => 15,
    };
    writer.byte(tag)?;
    match reason {
        ClaimRejection::ValueTooLarge { bytes, maximum } => {
            writer.u64(u64::try_from(bytes).map_err(|_| CodecError::LengthOverflow)?)?;
            writer.u64(u64::try_from(maximum).map_err(|_| CodecError::LengthOverflow)?)
        }
        ClaimRejection::WorkBytes { bytes, maximum } => {
            writer.u64(bytes)?;
            writer.u64(maximum)
        }
        _ => Ok(()),
    }
}

fn read_rejection(reader: &mut Reader<'_>) -> Result<ClaimRejection, CodecError> {
    Ok(match reader.byte()? {
        0 => ClaimRejection::InvalidScope,
        1 => ClaimRejection::InvalidPriority,
        2 => ClaimRejection::InvalidBudget,
        3 => ClaimRejection::HeadBudget,
        4 => ClaimRejection::StaleContinuation,
        5 => ClaimRejection::EmptyType,
        6 => ClaimRejection::EmptyWorker,
        7 => ClaimRejection::ZeroLease,
        8 => ClaimRejection::LeaseTooLarge,
        9 => ClaimRejection::LeaseDeadlineTooLarge,
        10 => ClaimRejection::VersionOverflow,
        11 => ClaimRejection::FenceOverflow,
        12 => ClaimRejection::ValueTooLarge {
            bytes: usize::try_from(reader.u64()?).map_err(|_| CodecError::LengthOverflow)?,
            maximum: usize::try_from(reader.u64()?).map_err(|_| CodecError::LengthOverflow)?,
        },
        13 => ClaimRejection::DecisionBytes,
        14 => ClaimRejection::LengthOverflow,
        15 => ClaimRejection::WorkBytes {
            bytes: reader.u64()?,
            maximum: reader.u64()?,
        },
        _ => return Err(CodecError::Malformed),
    })
}
