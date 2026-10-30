//! Core record wire fields and complete borrowed validation before new owners.

use wf_domain::history::EventKind;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, record_bytes};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId};

use super::{StateValueError, add, allocate, put_raw, raw_size};

mod scan;
pub use scan::{ScannedWorkflowRecord, scan_record_point, scan_record_point_profiled};

/// Encodes explicit v2 represented fields; selecting data grants no store upgrade.
/// # Errors
/// Rejects field/profile, format, allocation or complete encoded-byte limits.
pub fn encode_record_snapshot(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_fields_version(
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
        2,
    )
}

/// Scans explicit v2 bytes at one exact point without storage authority.
/// # Errors
/// Rejects mismatched format/point, malformed bytes and field or encoded caps.
pub fn scan_record_snapshot<'input>(
    bytes: &'input [u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<ScannedWorkflowRecord<'input>, StateValueError> {
    scan::scan(
        bytes,
        scope.group(),
        scope.recovery(),
        Some((scope.tenant(), partition, id)),
        limits,
        maximum_bytes,
        (true, 2),
    )
}

pub(in crate::state::value) struct RecordHistoryView<'a> {
    pub(in crate::state::value) state: &'a [u8],
    pub(in crate::state::value) version: u64,
    pub(in crate::state::value) fence: u64,
    pub(in crate::state::value) attempts: u64,
    pub(in crate::state::value) updated: u64,
    pub(in crate::state::value) next: Option<u64>,
    pub(in crate::state::value) worker: Option<&'a [u8]>,
    pub(in crate::state::value) deadline: u64,
    pub(in crate::state::value) latest: (EventKind, u64, u64),
}

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
    encode_fields_version(namespace, record, ordering, limits, maximum_bytes, 1)
}

fn encode_fields_version(
    namespace: [&[u8]; 4],
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    limits: AdmissionLimits,
    maximum_bytes: usize,
    format: u16,
) -> Result<Vec<u8>, StateValueError> {
    let size = measure_fields(namespace, record, ordering, limits, maximum_bytes)?;
    let mut output = allocate(size, maximum_bytes, *b"FWSR")?;
    output[4..6].copy_from_slice(&format.to_be_bytes());
    write_body(&mut output, namespace, record, ordering, format)?;
    Ok(output)
}

pub(super) fn measure_fields(
    namespace: [&[u8]; 4],
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<usize, StateValueError> {
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
    Ok(size)
}

pub(super) fn write_fields(
    output: &mut Vec<u8>,
    namespace: [&[u8]; 4],
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    format: u16,
) -> Result<(), StateValueError> {
    output.extend_from_slice(b"FWSR");
    output.extend_from_slice(&format.to_be_bytes());
    write_body(output, namespace, record, ordering, format)
}

fn write_body(
    output: &mut Vec<u8>,
    namespace: [&[u8]; 4],
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    format: u16,
) -> Result<(), StateValueError> {
    for field in namespace {
        put_raw(output, field)?;
    }
    output.push(ordering.priority);
    output.extend_from_slice(&ordering.state_enter_seq.to_be_bytes());
    put_record(output, record, format)
}

fn put_record(
    output: &mut Vec<u8>,
    record: &WorkflowRecord,
    format: u16,
) -> Result<(), StateValueError> {
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
    output.push(event_tag_version(kind, format)?);
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

pub(super) fn event_tag_version(kind: EventKind, format: u16) -> Result<u8, StateValueError> {
    match format {
        2 if kind == EventKind::StepContinued => Ok(13),
        1 | 2 => event_tag(kind),
        _ => Err(StateValueError::UnsupportedVersion),
    }
}

pub(super) fn event_version(tag: u8, format: u16) -> Result<EventKind, StateValueError> {
    match format {
        2 if tag == 13 => Ok(EventKind::StepContinued),
        1 | 2 => event(tag),
        _ => Err(StateValueError::UnsupportedVersion),
    }
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

#[cfg(test)]
#[test]
fn step_continue_event_keeps_storage_wire_closed() {
    assert_eq!(
        event_tag(EventKind::StepContinued),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(event(13), Err(StateValueError::UnsupportedTag));
    assert_eq!(event_tag(EventKind::Created), Ok(0));
    assert_eq!(event(0), Ok(EventKind::Created));
    assert_eq!(event_tag(EventKind::ChildrenSpawned), Ok(12));
    assert_eq!(event(12), Ok(EventKind::ChildrenSpawned));
}
