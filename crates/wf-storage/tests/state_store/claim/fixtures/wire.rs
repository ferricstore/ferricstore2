//! Independent persisted-value and full Claim receipt byte packing.

use super::{TestResult, key, optional, raw};
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::workflow::request::{RequestScope, RetainedMetadata};
use wf_ports::state::StoredWorkflowRecord;

fn event(kind: EventKind) -> TestResult<u8> {
    match kind {
        EventKind::Created => Ok(0),
        EventKind::Claimed => Ok(1),
        EventKind::Completed => Ok(3),
        EventKind::Cancelled => Ok(7),
        _ => Err("oracle event not named by these fixtures".into()),
    }
}

pub(in super::super) fn record_wire(
    scope: &RequestScope,
    owner: &StoredWorkflowRecord,
) -> TestResult<Vec<u8>> {
    let row = &owner.record;
    let mut bytes = b"FWSR\0\x01".to_vec();
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        owner.partition.as_bytes(),
    ] {
        raw(&mut bytes, field)?;
    }
    bytes.push(owner.ordering.priority);
    bytes.extend_from_slice(&owner.ordering.state_enter_seq.to_be_bytes());
    for field in [row.id().as_bytes(), row.workflow_type(), row.state()] {
        raw(&mut bytes, field)?;
    }
    for value in [
        row.version(),
        row.fence().get(),
        row.attempts(),
        row.created_at().get(),
        row.updated_at().get(),
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.push(u8::from(row.next_run_at().is_some()));
    if let Some(time) = row.next_run_at() {
        bytes.extend_from_slice(&time.get().to_be_bytes());
    }
    optional(&mut bytes, row.run_state())?;
    bytes.push(u8::from(row.lease().is_some()));
    if let Some(lease) = row.lease() {
        raw(&mut bytes, &lease.worker)?;
        raw(&mut bytes, lease.token.as_bytes())?;
        bytes.extend_from_slice(&lease.deadline.get().to_be_bytes());
    }
    for value in [row.payload(), row.result(), row.error()] {
        optional(&mut bytes, value)?;
    }
    let (kind, version, fence) = row.latest_event_witness();
    bytes.push(event(kind)?);
    bytes.extend_from_slice(&version.to_be_bytes());
    bytes.extend_from_slice(&fence.get().to_be_bytes());
    Ok(bytes)
}

pub(in super::super) fn history_wire(
    scope: &RequestScope,
    owner: &StoredWorkflowRecord,
    history: &HistoryEntry,
) -> TestResult<(Vec<u8>, Vec<u8>)> {
    assert!(history.retry.is_none());
    let mut bytes = b"FWSH\0\x01".to_vec();
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        owner.partition.as_bytes(),
        owner.record.id().as_bytes(),
        history.event_id.as_bytes(),
    ] {
        raw(&mut bytes, field)?;
    }
    bytes.push(event(history.kind)?);
    for number in [
        history.occurred_at.get(),
        history.version,
        history.fence.get(),
        history.attempts,
    ] {
        bytes.extend_from_slice(&number.to_be_bytes());
    }
    raw(&mut bytes, &history.state)?;
    bytes.push(u8::from(history.next_run_at.is_some()));
    if let Some(time) = history.next_run_at {
        bytes.extend_from_slice(&time.get().to_be_bytes());
    }
    bytes.extend_from_slice(&history.lease_deadline.get().to_be_bytes());
    optional(&mut bytes, history.worker.as_deref())?;
    bytes.push(0);
    let mut physical = key(
        5,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            owner.partition.as_bytes(),
            owner.record.id().as_bytes(),
        ],
    );
    physical.extend_from_slice(&history.occurred_at.get().to_be_bytes());
    physical.extend_from_slice(&key(5, &[history.event_id.as_bytes()])[7..]);
    Ok((physical, bytes))
}

pub(in super::super) fn receipt(
    owners: &[StoredWorkflowRecord],
    stop: u8,
    scanned: u32,
    heads: u32,
) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWCR\0\x01\0".to_vec();
    bytes.extend_from_slice(&[stop, u8::from(stop != 0)]);
    for count in [scanned, heads, u32::try_from(owners.len())?] {
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    for owner in owners {
        let row = &owner.record;
        let lease = row.lease().ok_or("receipt fixture has no explicit lease")?;
        for field in [
            owner.partition.as_bytes(),
            row.id().as_bytes(),
            row.workflow_type(),
            row.run_state().ok_or("receipt fixture has no run state")?,
            &lease.worker,
            lease.token.as_bytes(),
        ] {
            raw(&mut bytes, field)?;
        }
        for value in [
            row.version(),
            row.fence().get(),
            lease.deadline.get(),
            row.created_at().get(),
            row.updated_at().get(),
            row.attempts(),
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.push(owner.ordering.priority);
        bytes.extend_from_slice(&owner.ordering.state_enter_seq.to_be_bytes());
        for value in [row.payload(), row.result(), row.error()] {
            optional(&mut bytes, value)?;
        }
    }
    Ok(bytes)
}

pub(in super::super) fn outcome_wire(
    metadata: &RetainedMetadata,
    response: &[u8],
) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWSO\0\x01".to_vec();
    for field in [
        metadata.scope.tenant().as_bytes(),
        metadata.scope.group().as_bytes(),
        metadata.scope.recovery().as_bytes(),
        metadata.request.as_bytes(),
    ] {
        raw(&mut bytes, field)?;
    }
    bytes.extend_from_slice(&metadata.intent.version().to_be_bytes());
    bytes.extend_from_slice(metadata.intent.digest());
    for number in [
        metadata.original_position.term(),
        metadata.original_position.leader_node(),
        metadata.original_position.index().get(),
        metadata.original_time.get(),
        metadata.expires_at.get(),
    ] {
        bytes.extend_from_slice(&number.to_be_bytes());
    }
    bytes.extend_from_slice(&3_u16.to_be_bytes());
    raw(&mut bytes, response)?;
    Ok(bytes)
}
