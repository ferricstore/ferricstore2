//! Independent persisted, policy, manifest, receipt and snapshot byte packing.

use super::{Cohort, TestResult, optional, raw};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::request::{RequestScope, RetainedMetadata};
use wf_ports::snapshot::SnapshotTransition;
use wf_ports::state::StoredWorkflowRecord;
use wf_types::version::{CommittedTime, LogIdentity};

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
    response_version: u16,
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
    bytes.extend_from_slice(&response_version.to_be_bytes());
    raw(&mut bytes, response)?;
    Ok(bytes)
}

pub(in super::super) fn key(family: u8, fields: &[&[u8]]) -> Vec<u8> {
    let mut out = b"FWPK\0\x01".to_vec();
    out.push(family);
    for field in fields {
        for byte in *field {
            out.push(*byte);
            if *byte == 0 {
                out.push(255);
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

pub(in super::super) fn point(scope: &RequestScope, owner: &StoredWorkflowRecord) -> Vec<u8> {
    key(
        1,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            owner.partition.as_bytes(),
            owner.record.id().as_bytes(),
        ],
    )
}

pub(in super::super) fn due(
    scope: &RequestScope,
    owner: &StoredWorkflowRecord,
) -> TestResult<Vec<u8>> {
    let row = &owner.record;
    let mut out = key(
        3,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
    );
    out.push(2 - owner.ordering.priority);
    out.extend_from_slice(
        &row.next_run_at()
            .ok_or("missing expected due time")?
            .get()
            .to_be_bytes(),
    );
    out.extend_from_slice(
        &key(
            3,
            &[
                row.id().as_bytes(),
                owner.partition.as_bytes(),
                row.run_state().unwrap_or(row.state()),
                row.workflow_type(),
            ],
        )[7..],
    );
    Ok(out)
}

pub(in super::super) fn member(scope: &RequestScope, owner: &StoredWorkflowRecord) -> Vec<u8> {
    let row = &owner.record;
    let mut out = key(
        4,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            row.workflow_type(),
            row.run_state().unwrap_or(row.state()),
            owner.partition.as_bytes(),
        ],
    );
    out.push(u8::from(row.state() != b"running"));
    out.extend_from_slice(&owner.ordering.state_enter_seq.to_be_bytes());
    out.extend_from_slice(&key(4, &[row.id().as_bytes()])[7..]);
    out
}

pub(in super::super) fn policy_wire(
    kind: &[u8],
    generation: u64,
    states: &[(&[u8], StateMode)],
    rich: bool,
) -> TestResult<Vec<u8>> {
    let mut snapshot = b"FWPS\0\x02".to_vec();
    raw(&mut snapshot, kind)?;
    if rich {
        snapshot.extend_from_slice(&[1, 1]); // Present numeric customer version zero.
        snapshot.extend_from_slice(&0_u64.to_be_bytes());
        snapshot.push(1);
        snapshot.extend_from_slice(&1_000_u64.to_be_bytes());
        snapshot.extend_from_slice(&[1, 1, 0, 7, 1, 1]); // Retry max7 and fixed backoff.
        for number in [17_u64, 33] {
            snapshot.extend_from_slice(&number.to_be_bytes());
        }
        snapshot.extend_from_slice(&[4, 1]);
        raw(&mut snapshot, b"dead")?;
        snapshot.extend_from_slice(&[1, 1]);
        snapshot.extend_from_slice(&1_u64.to_be_bytes());
        snapshot.push(1);
        snapshot.extend_from_slice(&64_u32.to_be_bytes());
    } else {
        snapshot.extend_from_slice(&[0; 4]);
    }
    snapshot.extend_from_slice(&u32::try_from(states.len())?.to_be_bytes());
    for (state, mode) in states {
        raw(&mut snapshot, state)?;
        snapshot.extend_from_slice(&[1, u8::from(*mode == StateMode::Fifo)]);
        if rich {
            snapshot.extend_from_slice(&[1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        } else {
            snapshot.extend_from_slice(&[0; 3]);
        }
    }
    if rich {
        snapshot.push(1);
        snapshot.extend_from_slice(&2_u32.to_be_bytes());
        for field in [b"a".as_slice(), b"z"] {
            raw(&mut snapshot, field)?;
        }
        snapshot.push(1);
        raw(&mut snapshot, b"m")?;
        snapshot.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    } else {
        snapshot.extend_from_slice(&[0; 3]);
    }
    let mut out = b"FWPV\0\x02".to_vec();
    out.extend_from_slice(&generation.to_be_bytes());
    raw(&mut out, &snapshot)?;
    Ok(out)
}

fn time(out: &mut Vec<u8>, value: CommittedTime) {
    out.extend_from_slice(&value.hlc().physical().get().to_be_bytes());
    out.extend_from_slice(&value.hlc().logical().to_be_bytes());
    out.extend_from_slice(&value.wall().get().to_be_bytes());
}

pub(in super::super) fn control_wire(
    scope: &RequestScope,
    frame: &[u8],
    at: LogIdentity,
    times: (CommittedTime, CommittedTime),
) -> TestResult<Vec<u8>> {
    let mut out = b"FWSC\0\x01".to_vec();
    raw(&mut out, scope.group().as_bytes())?;
    raw(&mut out, scope.recovery().as_bytes())?;
    time(&mut out, times.0);
    out.push(1);
    for number in [at.term(), at.leader_node(), at.index().get()] {
        out.extend_from_slice(&number.to_be_bytes());
    }
    out.extend_from_slice(&Sha256::digest(frame));
    out.push(1);
    time(&mut out, times.1);
    Ok(out)
}

pub(in super::super) fn finish(
    cohort: &mut Cohort,
    scope: &RequestScope,
    frame: &[u8],
    at: LogIdentity,
    times: (CommittedTime, CommittedTime),
    new: Option<(&RetainedMetadata, u16, &[u8])>,
) -> TestResult {
    let rows = cohort[1].as_mut().ok_or("no expected live rows")?;
    rows.insert(
        key(0, &[scope.group().as_bytes(), scope.recovery().as_bytes()]),
        control_wire(scope, frame, at, times)?,
    );
    if let Some((metadata, version, response)) = new {
        rows.insert(
            key(
                2,
                &[
                    scope.group().as_bytes(),
                    scope.recovery().as_bytes(),
                    scope.tenant().as_bytes(),
                    metadata.request.as_bytes(),
                ],
            ),
            outcome_wire(metadata, version, response)?,
        );
    }
    Ok(())
}

pub(in super::super) fn catalog(
    scope: &RequestScope,
    slots: [Option<([u8; 32], usize, usize)>; 2],
    current: Option<u8>,
    anchor: Option<u8>,
    pending: Option<SnapshotTransition>,
) -> TestResult<Vec<u8>> {
    let mut out = b"FWSD\0\x03".to_vec();
    out.extend_from_slice(&[current.map_or(0, |v| v + 1), anchor.map_or(0, |v| v + 1)]);
    for slot in slots {
        out.push(u8::from(slot.is_some()));
        if let Some((id, rows, bytes)) = slot {
            out.extend_from_slice(&u64::try_from(rows)?.to_be_bytes());
            out.extend_from_slice(&u64::try_from(bytes)?.to_be_bytes());
            out.extend_from_slice(&id);
        }
    }
    out.push(u8::from(pending.is_some()));
    if let Some(transition) = pending {
        let before = transition.before();
        out.extend_from_slice(&[
            u8::from(transition.is_received()),
            u8::from(before.vote().is_some())
                | (u8::from(before.committed().is_some()) << 1)
                | (u8::from(before.last_log().is_some()) << 2)
                | (u8::from(before.last_purged().is_some()) << 3),
        ]);
        if let Some(vote) = before.vote() {
            out.extend_from_slice(&vote.term().to_be_bytes());
            out.extend_from_slice(&vote.node().to_be_bytes());
            out.push(u8::from(vote.committed()));
        }
        for id in [before.committed(), before.last_log()]
            .into_iter()
            .flatten()
        {
            identity(&mut out, id);
        }
        if let Some((position, id)) = before.last_purged().zip(before.purged_snapshot_id()) {
            identity(&mut out, position);
            out.extend_from_slice(&id);
        }
        identity(&mut out, transition.position());
        out.extend_from_slice(transition.id());
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-catalog-v1");
    for field in [scope.group().as_bytes(), scope.recovery().as_bytes()] {
        hash.update(u16::try_from(field.len())?.to_be_bytes());
        hash.update(field);
    }
    hash.update(&out);
    out.extend_from_slice(&hash.finalize());
    Ok(out)
}

pub(in super::super) fn upgrade5(
    cohort: &mut Cohort,
    scope: &RequestScope,
    descriptor: Vec<u8>,
) -> TestResult {
    let (metadata, data) = cohort.split_at_mut(1);
    let rows = data[0].as_ref().ok_or("no expected current rows")?;
    let meta = metadata[0].as_mut().ok_or("no expected current metadata")?;
    let origin = if let Some(origin) = meta.get(b"origin".as_slice()) {
        origin.clone()
    } else {
        let mut origin = meta
            .get(b"manifest".as_slice())
            .ok_or("no birth manifest")?
            .clone();
        origin[4..6].copy_from_slice(&3_u16.to_be_bytes());
        let end = origin.len() - 32;
        let checksum = Sha256::digest(&origin[..end]);
        origin[end..].copy_from_slice(&checksum);
        origin
    };
    let mut manifest = origin[..origin.len() - 80].to_vec();
    manifest[4..6].copy_from_slice(&5_u16.to_be_bytes());
    let policies = rows
        .keys()
        .filter(|key| key.starts_with(b"FWPK\0\x01\x06"))
        .count();
    let placements = rows
        .keys()
        .filter(|key| key.starts_with(b"FWPK\0\x01\x07"))
        .count();
    for count in [placements, policies] {
        manifest.extend_from_slice(&u64::try_from(count)?.to_be_bytes());
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-state-current-catalog-v1");
    for (key, value) in rows
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(6 | 7)))
    {
        hash.update(u64::try_from(key.len())?.to_be_bytes());
        hash.update(key);
        hash.update(u64::try_from(value.len())?.to_be_bytes());
        hash.update(value);
    }
    manifest.extend_from_slice(&hash.finalize());
    manifest.extend_from_slice(&Sha256::digest(&origin));
    manifest.extend_from_slice(&Sha256::digest(&manifest));
    assert_eq!(
        manifest.len(),
        140 + scope.group().as_bytes().len() + scope.recovery().as_bytes().len()
    );
    meta.insert(b"origin".to_vec(), origin);
    meta.insert(b"manifest".to_vec(), manifest);
    meta.insert(b"snapshot".to_vec(), descriptor);
    for slot in &mut cohort[4..] {
        if slot.is_none() {
            *slot = Some(BTreeMap::new());
        }
    }
    Ok(())
}

pub(in super::super) fn snapshot_wire(
    scope: &RequestScope,
    cohort: &Cohort,
    version: u16,
) -> TestResult<Vec<u8>> {
    let mut meta = cohort[0].as_ref().ok_or("no live metadata")?.clone();
    meta.remove(b"snapshot".as_slice())
        .ok_or("missing descriptor")?;
    if version == 1 {
        let manifest = meta
            .get_mut(b"manifest".as_slice())
            .ok_or("missing manifest")?;
        manifest[4..6].copy_from_slice(&3_u16.to_be_bytes());
        let end = manifest.len() - 32;
        let checksum = Sha256::digest(&manifest[..end]);
        manifest[end..].copy_from_slice(&checksum);
    }
    assert_eq!(meta.len(), if version == 1 { 2 } else { 3 });
    let rows = cohort[1].as_ref().ok_or("no live rows")?;
    let bytes = 66
        + scope.group().as_bytes().len()
        + scope.recovery().as_bytes().len()
        + 12 * (meta.len() + rows.len())
        + meta
            .iter()
            .chain(rows)
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>();
    let mut out = b"FWSP".to_vec();
    out.extend_from_slice(&version.to_be_bytes());
    for field in [scope.group().as_bytes(), scope.recovery().as_bytes()] {
        out.extend_from_slice(&u16::try_from(field.len())?.to_be_bytes());
        out.extend_from_slice(field);
    }
    for count in [meta.len(), rows.len(), bytes] {
        out.extend_from_slice(&u64::try_from(count)?.to_be_bytes());
    }
    for (key, value) in meta.iter().chain(rows) {
        out.extend_from_slice(&u32::try_from(key.len())?.to_be_bytes());
        out.extend_from_slice(&u64::try_from(value.len())?.to_be_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    out.extend_from_slice(&Sha256::digest(&out));
    assert_eq!(out.len(), bytes);
    Ok(out)
}

fn identity(out: &mut Vec<u8>, id: LogIdentity) {
    for number in [id.term(), id.leader_node(), id.index().get()] {
        out.extend_from_slice(&number.to_be_bytes());
    }
}
