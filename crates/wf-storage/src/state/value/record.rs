//! Core record wire fields and complete borrowed validation before new owners.

use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::admission::{AdmissionLimits, record_bytes};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{MAX_EXACT_MILLIS, WorkflowRecord};
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

use super::{
    StateValueError, add, allocate, copy, finish, flag, put_raw, raw, raw_size, read_u8, read_u64,
    read_u128, start,
};

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
    if maximum_bytes == 0 || owner.ordering.priority > 2 {
        return Err(StateValueError::InvalidValue);
    }
    record_bytes(&owner.record, limits).map_err(|_| StateValueError::InvalidValue)?;
    let record = &owner.record;
    let mut size = 80;
    for field in [
        group.as_bytes(),
        recovery.as_bytes(),
        owner.tenant.as_bytes(),
        owner.partition.as_bytes(),
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
    for field in [
        group.as_bytes(),
        recovery.as_bytes(),
        owner.tenant.as_bytes(),
        owner.partition.as_bytes(),
    ] {
        put_raw(&mut output, field)?;
    }
    output.push(owner.ordering.priority);
    output.extend_from_slice(&owner.ordering.state_enter_seq.to_be_bytes());
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

fn optional_raw<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
) -> Result<Option<&'a [u8]>, StateValueError> {
    if flag(bytes, offset)? {
        Ok(Some(raw(bytes, offset)?))
    } else {
        Ok(None)
    }
}

fn optional_time(bytes: &[u8], offset: &mut usize) -> Result<Option<u64>, StateValueError> {
    if flag(bytes, offset)? {
        Ok(Some(read_u64(bytes, offset)?))
    } else {
        Ok(None)
    }
}

pub(super) fn event(tag: u8) -> Result<EventKind, StateValueError> {
    EVENTS
        .get(usize::from(tag))
        .copied()
        .ok_or(StateValueError::UnsupportedTag)
}

fn nonempty(bytes: &[u8], maximum: usize) -> bool {
    !bytes.is_empty() && bytes.len() <= maximum
}

fn validate(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: AdmissionLimits,
    maximum: usize,
) -> Result<(), StateValueError> {
    let mut offset = start(bytes, *b"FWSR", maximum)?;
    let actual_group = raw(bytes, &mut offset)?;
    let actual_recovery = raw(bytes, &mut offset)?;
    let tenant = raw(bytes, &mut offset)?;
    let partition = raw(bytes, &mut offset)?;
    let priority = read_u8(bytes, &mut offset)?;
    let _ = read_u128(bytes, &mut offset)?;
    let id = raw(bytes, &mut offset)?;
    let workflow_type = raw(bytes, &mut offset)?;
    let state = raw(bytes, &mut offset)?;
    let version = read_u64(bytes, &mut offset)?;
    let fence = read_u64(bytes, &mut offset)?;
    let _ = read_u64(bytes, &mut offset)?;
    let created = read_u64(bytes, &mut offset)?;
    let updated = read_u64(bytes, &mut offset)?;
    let next = optional_time(bytes, &mut offset)?;
    let run_state = optional_raw(bytes, &mut offset)?;
    let lease = if flag(bytes, &mut offset)? {
        Some((
            raw(bytes, &mut offset)?,
            raw(bytes, &mut offset)?,
            read_u64(bytes, &mut offset)?,
        ))
    } else {
        None
    };
    let payload = optional_raw(bytes, &mut offset)?;
    let result = optional_raw(bytes, &mut offset)?;
    let error = optional_raw(bytes, &mut offset)?;
    let latest = read_u8(bytes, &mut offset)?;
    let latest_version = read_u64(bytes, &mut offset)?;
    let latest_fence = read_u64(bytes, &mut offset)?;
    finish(bytes, offset)?;
    event(latest)?;
    if actual_group != group.as_bytes()
        || actual_recovery != recovery.as_bytes()
        || !nonempty(tenant, 8_192)
        || !nonempty(partition, 65_535)
        || !nonempty(id, 65_535)
        || priority > 2
        || workflow_type.is_empty()
        || state.is_empty()
        || version == 0
        || latest_version != version
        || latest_fence != fence
        || created > MAX_EXACT_MILLIS
        || updated > MAX_EXACT_MILLIS
        || run_state.is_some_and(<[u8]>::is_empty)
        || (matches!(state, b"completed" | b"failed" | b"cancelled") && next.is_some())
        || lease.is_some_and(|(worker, token, deadline)| {
            worker.is_empty()
                || token.is_empty()
                || deadline > MAX_EXACT_MILLIS
                || state != b"running"
                || next != Some(deadline)
        })
    {
        return Err(StateValueError::InvalidValue);
    }
    // Borrowed wire-field admission precedes owners. restore_record repeats its
    // source consistency/admission checks after the fields move, without cloning.
    let mut total = id.len();
    for field in [
        Some(workflow_type),
        Some(state),
        run_state,
        payload,
        result,
        error,
        lease.map(|value| value.0),
        lease.map(|value| value.1),
    ]
    .into_iter()
    .flatten()
    {
        if field.len() > limits.inline_bytes() {
            return Err(StateValueError::InvalidValue);
        }
        add(&mut total, field.len())?;
    }
    if total > limits.decision_bytes() {
        return Err(StateValueError::InvalidValue);
    }
    Ok(())
}

/// Validates complete framing, namespace and borrowed fields before owned decode.
///
/// # Errors
/// Rejects unsupported/corrupt values, explicit caps, admission or allocation.
pub fn decode_record(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<StoredWorkflowRecord, StateValueError> {
    validate(bytes, group, recovery, limits, maximum_bytes)?;
    let mut offset = 6;
    raw(bytes, &mut offset)?;
    raw(bytes, &mut offset)?;
    let tenant = TenantId::new(copy(raw(bytes, &mut offset)?)?)
        .map_err(|_| StateValueError::InvalidValue)?;
    let partition = PartitionId::new(copy(raw(bytes, &mut offset)?)?)
        .map_err(|_| StateValueError::InvalidValue)?;
    let ordering = OrderingMetadata {
        priority: read_u8(bytes, &mut offset)?,
        state_enter_seq: read_u128(bytes, &mut offset)?,
    };
    let id = RecordId::new(copy(raw(bytes, &mut offset)?)?)
        .map_err(|_| StateValueError::InvalidValue)?;
    let workflow_type = copy(raw(bytes, &mut offset)?)?;
    let state = copy(raw(bytes, &mut offset)?)?;
    let version = read_u64(bytes, &mut offset)?;
    let fence = Fence::new(read_u64(bytes, &mut offset)?);
    let attempts = read_u64(bytes, &mut offset)?;
    let created_at = TimestampMillis::new(read_u64(bytes, &mut offset)?);
    let updated_at = TimestampMillis::new(read_u64(bytes, &mut offset)?);
    let next_run_at = optional_time(bytes, &mut offset)?.map(TimestampMillis::new);
    let run_state = optional_raw(bytes, &mut offset)?.map(copy).transpose()?;
    let lease = if flag(bytes, &mut offset)? {
        Some(Lease {
            worker: copy(raw(bytes, &mut offset)?)?,
            token: LeaseToken::new(copy(raw(bytes, &mut offset)?)?)
                .map_err(|_| StateValueError::InvalidValue)?,
            deadline: TimestampMillis::new(read_u64(bytes, &mut offset)?),
        })
    } else {
        None
    };
    let payload = optional_raw(bytes, &mut offset)?.map(copy).transpose()?;
    let result = optional_raw(bytes, &mut offset)?.map(copy).transpose()?;
    let error = optional_raw(bytes, &mut offset)?.map(copy).transpose()?;
    let latest_kind = event(read_u8(bytes, &mut offset)?)?;
    let latest_version = read_u64(bytes, &mut offset)?;
    let latest_fence = Fence::new(read_u64(bytes, &mut offset)?);
    let record = restore_record(
        WorkflowRecordData {
            id,
            workflow_type,
            state,
            version,
            fence,
            attempts,
            created_at,
            updated_at,
            next_run_at,
            run_state,
            lease,
            payload,
            result,
            error,
            latest_kind,
            latest_version,
            latest_fence,
        },
        limits,
    )
    .map_err(|_| StateValueError::InvalidValue)?;
    Ok(StoredWorkflowRecord {
        tenant,
        partition,
        record,
        ordering,
    })
}
