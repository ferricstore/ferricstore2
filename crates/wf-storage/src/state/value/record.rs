//! Core record wire fields and complete borrowed validation before new owners.

use wf_domain::history::EventKind;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, record_bytes};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::{GroupId, PartitionId, RecoveryId};

use super::{StateValueError, add, allocate, put_raw, raw_size};

mod scan;
pub use scan::{ScannedWorkflowRecord, scan_record_point, scan_record_point_profiled};

const EVENTS: [EventKind; 13] = [
    EventKind::Created,
    EventKind::Claimed,
    EventKind::Transitioned,
    EventKind::Completed,
    EventKind::Failed,
    EventKind::Retry,
    EventKind::LeaseExtended,
    EventKind::Cancelled,
    EventKind::ChildCompleted,
    EventKind::ChildFailed,
    EventKind::ChildCancelled,
    EventKind::ChildrenCancelled,
    EventKind::ChildrenSpawned,
];

/// Measures/admit an immutable core record, then emits one versioned owner.
///
/// # Errors
/// Rejects profile, ordering, record admission, wire limits or allocation failure.
pub fn encode_record(
    group: &GroupId,
    recovery: &RecoveryId,
    owner: &StoredWorkflowRecord,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_fields(
        [
            group.as_bytes(),
            recovery.as_bytes(),
            owner.tenant.as_bytes(),
            owner.partition.as_bytes(),
        ],
        &owner.record,
        owner.ordering,
        limits,
        maximum_bytes,
    )
}

/// Encodes borrowed proposed fields without constructing a second record owner.
/// # Errors
/// Preserves the legacy ordering, admission, wire-cap and allocation refusals.
pub fn encode_record_fields(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_fields(
        [
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
        ],
        record,
        ordering,
        limits,
        maximum_bytes,
    )
}

fn encode_fields(
    namespace: [&[u8]; 4],
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 || ordering.priority > 2 {
        return Err(StateValueError::InvalidValue);
    }
    record_bytes(record, limits).map_err(|_| StateValueError::InvalidValue)?;
    let mut size = 80;
    for field in [
        namespace[0],
        namespace[1],
        namespace[2],
        namespace[3],
        record.id().as_bytes(),
        record.workflow_type(),
        record.state(),
    ] {
        add(&mut size, raw_size(field)?)?;
    }
    add(
        &mut size,
        if record.next_run_at().is_some() { 9 } else { 1 },
    )?;
    for field in [
        record.run_state(),
        record.payload(),
        record.result(),
        record.error(),
    ] {
        add(&mut size, 1)?;
        if let Some(bytes) = field {
            add(&mut size, raw_size(bytes)?)?;
        }
    }
    add(&mut size, 1)?;
    if let Some(lease) = record.lease() {
        add(&mut size, raw_size(&lease.worker)?)?;
        add(&mut size, raw_size(lease.token.as_bytes())?)?;
        add(&mut size, 8)?;
    }
    let mut output = allocate(size, maximum_bytes, *b"FWSR")?;
    for field in namespace {
        put_raw(&mut output, field)?;
    }
    output.push(ordering.priority);
    output.extend_from_slice(&ordering.state_enter_seq.to_be_bytes());
    put_record(&mut output, record)?;
    Ok(output)
}

fn put_record(output: &mut Vec<u8>, record: &WorkflowRecord) -> Result<(), StateValueError> {
    for field in [
        record.id().as_bytes(),
        record.workflow_type(),
        record.state(),
    ] {
        put_raw(output, field)?;
    }
    for value in [
        record.version(),
        record.fence().get(),
        record.attempts(),
        record.created_at().get(),
        record.updated_at().get(),
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    output.push(u8::from(record.next_run_at().is_some()));
    if let Some(value) = record.next_run_at() {
        output.extend_from_slice(&value.get().to_be_bytes());
    }
    put_optional(output, record.run_state())?;
    output.push(u8::from(record.lease().is_some()));
    if let Some(lease) = record.lease() {
        put_raw(output, &lease.worker)?;
        put_raw(output, lease.token.as_bytes())?;
        output.extend_from_slice(&lease.deadline.get().to_be_bytes());
    }
    for value in [record.payload(), record.result(), record.error()] {
        put_optional(output, value)?;
    }
    let (kind, version, fence) = record.latest_event_witness();
    output.push(event_tag(kind)?);
    output.extend_from_slice(&version.to_be_bytes());
    output.extend_from_slice(&fence.get().to_be_bytes());
    Ok(())
}

pub(super) fn event_tag(kind: EventKind) -> Result<u8, StateValueError> {
    let tag = EVENTS
        .iter()
        .position(|event| *event == kind)
        .ok_or(StateValueError::InvalidValue)?;
    u8::try_from(tag).map_err(|_| StateValueError::LengthOverflow)
}

fn put_optional(output: &mut Vec<u8>, bytes: Option<&[u8]>) -> Result<(), StateValueError> {
    output.push(u8::from(bytes.is_some()));
    if let Some(bytes) = bytes {
        put_raw(output, bytes)?;
    }
    Ok(())
}

pub(super) fn event(tag: u8) -> Result<EventKind, StateValueError> {
    EVENTS
        .get(usize::from(tag))
        .copied()
        .ok_or(StateValueError::UnsupportedTag)
}

/// Validates complete framing, namespace and borrowed fields before owned decode.
///
/// This legacy entry preserves its group/recovery-only and wire-only cap contract.
/// # Errors
/// Rejects unsupported/corrupt values, explicit caps, admission or allocation.
pub fn decode_record(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<StoredWorkflowRecord, StateValueError> {
    scan::scan_legacy(bytes, group, recovery, limits, maximum_bytes)?.into_owned(usize::MAX)
}
