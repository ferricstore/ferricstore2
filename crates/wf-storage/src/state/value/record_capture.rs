//! Complete original metadata and core data, with exact physical association.

use super::{StateValueError, allocate, finish, put_raw, raw, raw_size, read_u128, start, take};
use sha2::{Digest, Sha256};
use wf_domain::history::EventKind;
mod model;
pub use model::{
    RecordCaptureError, RecordCaptureInput, RecordCaptureLimits, RecordCapturePoint,
    ScannedRecordCapture,
};

/// Encodes one complete data envelope; this grants no publication authority.
/// # Errors
/// Refuses invalid identity/record or separate byte/work/temporary ceilings.
pub fn encode_record_capture(
    input: RecordCaptureInput<'_>,
    limits: RecordCaptureLimits,
) -> Result<Vec<u8>, RecordCaptureError> {
    validate_limits(limits)?;
    let metadata = input.metadata.as_stored_bytes();
    admit_bytes(metadata.len(), limits.maximum_metadata_bytes)?;
    validate_birth(
        input.record.latest_event_witness().0,
        input.incarnation,
        input.ordering.state_enter_seq,
    )?;
    let namespace = [
        input.scope.group().as_bytes(),
        input.scope.recovery().as_bytes(),
        input.scope.tenant().as_bytes(),
        input.partition.as_bytes(),
    ];
    let core_size = super::record::measure_fields(
        namespace,
        input.record,
        input.ordering,
        limits.admission,
        limits.maximum_encoded_bytes,
    )?;
    let core_length = u32::try_from(core_size).map_err(|_| StateValueError::LengthOverflow)?;
    let size = total(54, &[4, core_size, raw_size(metadata)?])?;
    admit_bytes(size, limits.maximum_encoded_bytes)?;
    admit_work(size, limits.maximum_work_units)?;
    admit_temporary(total(CONTROL, &[size])?, limits.maximum_temporary_bytes)?;
    let mut output = allocate(size, limits.maximum_encoded_bytes, *b"FWMC")?;
    output.extend_from_slice(&input.incarnation.to_be_bytes());
    output.extend_from_slice(&core_length.to_be_bytes());
    super::record::write_fields(&mut output, namespace, input.record, input.ordering, 2)?;
    put_raw(&mut output, metadata)?;
    if output.len().checked_add(32) != Some(size) {
        return Err(StateValueError::InvalidValue.into());
    }
    let digest: [u8; 32] = Sha256::digest(&output).into();
    output.extend_from_slice(&digest);
    Ok(output)
}

/// Borrows a complete envelope at the exact expected point and birth/version.
/// Metadata remains opaque until decoded with explicit compatibility context.
/// # Errors
/// Refuses malformed/checksum/association/profile or independent resource limits.
pub fn scan_record_capture<'a>(
    bytes: &'a [u8],
    point: RecordCapturePoint<'_>,
    limits: RecordCaptureLimits,
) -> Result<ScannedRecordCapture<'a>, RecordCaptureError> {
    validate_limits(limits)?;
    admit_bytes(bytes.len(), limits.maximum_encoded_bytes)?;
    let work = admit_work(bytes.len(), limits.maximum_work_units)?;
    admit_temporary(CONTROL, limits.maximum_temporary_bytes)?;
    let mut at = start(bytes, *b"FWMC", limits.maximum_encoded_bytes)?;
    let incarnation = read_u128(bytes, &mut at)?;
    if incarnation != point.incarnation {
        return Err(StateValueError::InvalidValue.into());
    }
    let core_bytes = raw(bytes, &mut at)?;
    let metadata = raw(bytes, &mut at)?;
    admit_bytes(metadata.len(), limits.maximum_metadata_bytes)?;
    // This is only a stored operand discriminator, not a typed/schema promise.
    if !metadata.starts_with(b"FSV2\x02\x83") {
        return Err(StateValueError::InvalidValue.into());
    }
    let checksum_at = at;
    let checksum = take(bytes, &mut at, 32)?;
    finish(bytes, at)?;
    let actual: [u8; 32] = Sha256::digest(&bytes[..checksum_at]).into();
    if checksum != actual {
        return Err(StateValueError::InvalidValue.into());
    }
    let record = super::record::scan_record_snapshot(
        core_bytes,
        point.scope,
        point.partition,
        point.id,
        limits.admission,
        limits.maximum_encoded_bytes,
    )?;
    if record.version() != point.version {
        return Err(StateValueError::InvalidValue.into());
    }
    let (event, sequence) = record.capture_birth_fields();
    validate_birth(event, incarnation, sequence)?;
    Ok(ScannedRecordCapture {
        core: record,
        core_bytes,
        metadata,
        incarnation,
        work,
    })
}

const CONTROL: usize = 4096;

fn validate_limits(limits: RecordCaptureLimits) -> Result<(), RecordCaptureError> {
    if [
        limits.maximum_encoded_bytes,
        limits.maximum_metadata_bytes,
        limits.maximum_work_units,
        limits.maximum_temporary_bytes,
    ]
    .contains(&0)
    {
        return Err(RecordCaptureError::InvalidLimits);
    }
    Ok(())
}
fn total(initial: usize, amounts: &[usize]) -> Result<usize, StateValueError> {
    amounts.iter().try_fold(initial, |sum, amount| {
        sum.checked_add(*amount)
            .ok_or(StateValueError::LengthOverflow)
    })
}
fn admit_bytes(bytes: usize, maximum: usize) -> Result<(), StateValueError> {
    if bytes > maximum {
        Err(StateValueError::Limit { bytes, maximum })
    } else {
        Ok(())
    }
}
fn admit_work(bytes: usize, maximum: usize) -> Result<usize, RecordCaptureError> {
    let work = bytes
        .checked_mul(128)
        .and_then(|n| n.checked_add(CONTROL))
        .ok_or(RecordCaptureError::WorkLimit)?;
    if work > maximum {
        Err(RecordCaptureError::WorkLimit)
    } else {
        Ok(work)
    }
}
fn admit_temporary(bytes: usize, maximum: usize) -> Result<(), RecordCaptureError> {
    if bytes > maximum {
        Err(RecordCaptureError::TemporaryLimit)
    } else {
        Ok(())
    }
}
fn validate_birth(
    event: EventKind,
    incarnation: u128,
    sequence: u128,
) -> Result<(), StateValueError> {
    if event == EventKind::Created && incarnation != sequence {
        Err(StateValueError::InvalidValue)
    } else {
        Ok(())
    }
}
