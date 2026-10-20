//! Six-table observations and independent catalog/log bytes; no fake adapter.

use super::{SnapshotLimits, StateSnapshotStore, TestResult, f};
use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::path::Path;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::{
    log::{RaftLogState, RaftLogStore},
    snapshot::{SnapshotTransition, StateSnapshotTransitionStore},
};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};

pub(super) const TABLES: [&str; 6] = [
    "wf_state_meta_v1",
    "wf_state_rows_v1",
    "wf_state_snapshot_meta_v1",
    "wf_state_snapshot_rows_v1",
    "wf_state_snapshot_alt_meta_v1",
    "wf_state_snapshot_alt_rows_v1",
];

pub(super) fn cohort(dir: &Path) -> TestResult<[f::Rows; 6]> {
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let read = database.begin_read()?;
    let mut names = read
        .list_tables()?
        .map(|x| x.name().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = TABLES.map(str::to_owned);
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = std::array::from_fn(|_| f::Rows::new());
    for (rows, name) in result.iter_mut().zip(TABLES) {
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
        assert_eq!(u64::try_from(rows.len())?, table.len()?);
    }
    Ok(result)
}

pub(super) fn artifact(state: &mut RedbStateStore, capture: bool) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let metadata = if capture {
        state.capture_snapshot(&mut bytes, SnapshotLimits::default())?
    } else {
        state
            .current_snapshot(&mut bytes, SnapshotLimits::default())?
            .ok_or("no artifact")?
    };
    assert_eq!(metadata.id(), &facts(&bytes)?.0);
    assert_eq!(metadata.encoded_bytes(), bytes.len());
    Ok(bytes)
}

pub(super) fn by_id(state: &mut RedbStateStore, id: [u8; 32]) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let metadata = state
        .snapshot_by_id(id, &mut bytes, SnapshotLimits::default())?
        .ok_or("ID absent")?;
    assert_eq!(metadata.id(), &id);
    assert_eq!(metadata.encoded_bytes(), bytes.len());
    Ok(bytes)
}

pub(super) fn facts(bytes: &[u8]) -> TestResult<([u8; 32], usize, usize)> {
    assert_eq!(&bytes[..12], b"FWSP\0\x01\0\x01g\0\x01z");
    let read = |at| -> TestResult<usize> {
        Ok(usize::try_from(u64::from_be_bytes(
            bytes[at..at + 8].try_into()?,
        ))?)
    };
    assert_eq!(read(28)?, bytes.len());
    Ok((
        Sha256::digest(bytes).into(),
        read(12)? + read(20)?,
        bytes.len(),
    ))
}

fn put_id(bytes: &mut Vec<u8>, id: wf_types::version::LogIdentity) {
    for n in [id.term(), id.leader_node(), id.index().get()] {
        bytes.extend_from_slice(&n.to_be_bytes());
    }
}

pub(super) fn catalog(
    current: u8,
    anchor: u8,
    slots: [Option<([u8; 32], usize, usize)>; 2],
    pending: Option<SnapshotTransition>,
) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWSD\0\x03".to_vec();
    bytes.extend_from_slice(&[current, anchor]);
    for item in slots {
        bytes.push(u8::from(item.is_some()));
        if let Some((id, rows, wire)) = item {
            bytes.extend_from_slice(&u64::try_from(rows)?.to_be_bytes());
            bytes.extend_from_slice(&u64::try_from(wire)?.to_be_bytes());
            bytes.extend_from_slice(&id);
        }
    }
    bytes.push(u8::from(pending.is_some()));
    if let Some(t) = pending {
        bytes.push(u8::from(t.is_received()));
        let before = t.before();
        bytes.push(
            u8::from(before.vote().is_some())
                | u8::from(before.committed().is_some()) << 1
                | u8::from(before.last_log().is_some()) << 2
                | u8::from(before.last_purged().is_some()) << 3,
        );
        if let Some(vote) = before.vote() {
            bytes.extend_from_slice(&vote.term().to_be_bytes());
            bytes.extend_from_slice(&vote.node().to_be_bytes());
            bytes.push(u8::from(vote.committed()));
        }
        for id in [before.committed(), before.last_log()]
            .into_iter()
            .flatten()
        {
            put_id(&mut bytes, id);
        }
        if let Some((position, id)) = before.last_purged().zip(before.purged_snapshot_id()) {
            put_id(&mut bytes, position);
            bytes.extend_from_slice(&id);
        }
        put_id(&mut bytes, t.position());
        bytes.extend_from_slice(t.id());
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-catalog-v1\0\x01g\0\x01z");
    hash.update(&bytes);
    bytes.extend_from_slice(&hash.finalize());
    Ok(bytes)
}

pub(super) fn slot(data: &[f::Rows; 6], id: [u8; 32]) -> TestResult<usize> {
    let bytes = data[0]
        .get(b"snapshot".as_slice())
        .ok_or("catalog absent")?;
    let mut at = 8;
    for slot in 0..2 {
        let tag = bytes[at];
        at += 1;
        if tag == 1 {
            if bytes[at + 16..at + 48] == id {
                return Ok(slot);
            }
            at += 48;
        } else {
            assert_eq!(tag, 0);
        }
    }
    Err("artifact slot absent".into())
}

fn slot_facts(data: &[f::Rows; 6], slot: usize) -> TestResult<Option<([u8; 32], usize, usize)>> {
    let meta = &data[2 + 2 * slot];
    let rows = &data[3 + 2 * slot];
    if meta.is_empty() {
        assert!(rows.is_empty());
        return Ok(None);
    }
    assert_eq!(meta.len(), 2);
    let count = meta.len() + rows.len();
    let length = 68
        + 12 * count
        + meta
            .iter()
            .chain(rows)
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>();
    let mut bytes = b"FWSP\0\x01\0\x01g\0\x01z".to_vec();
    for n in [meta.len(), rows.len(), length] {
        bytes.extend_from_slice(&u64::try_from(n)?.to_be_bytes());
    }
    for (key, value) in meta.iter().chain(rows) {
        bytes.extend_from_slice(&u32::try_from(key.len())?.to_be_bytes());
        bytes.extend_from_slice(&u64::try_from(value.len())?.to_be_bytes());
        bytes.extend_from_slice(key);
        bytes.extend_from_slice(value);
    }
    let footer = Sha256::digest(&bytes);
    bytes.extend_from_slice(&footer);
    assert_eq!(bytes.len(), length);
    Ok(Some(facts(&bytes)?))
}

pub(super) fn pending(mut data: [f::Rows; 6], t: SnapshotTransition) -> TestResult<[f::Rows; 6]> {
    let before = data[0]
        .get(b"snapshot".as_slice())
        .ok_or("catalog missing")?;
    let anchor = before[7];
    let target = slot(&data, *t.id())?;
    let descriptor = catalog(
        u8::try_from(target + 1)?,
        anchor,
        [slot_facts(&data, 0)?, slot_facts(&data, 1)?],
        Some(t),
    )?;
    data[0].insert(b"snapshot".to_vec(), descriptor);
    Ok(data)
}

pub(super) fn finalized(mut data: [f::Rows; 6], target: usize) -> TestResult<[f::Rows; 6]> {
    let facts = slot_facts(&data, target)?.ok_or("target cohort absent")?;
    for slot in 0..2 {
        if slot != target {
            data[2 + 2 * slot].clear();
            data[3 + 2 * slot].clear();
        }
    }
    let mut slots = [None, None];
    slots[target] = Some(facts);
    let selector = u8::try_from(target + 1)?;
    data[0].insert(
        b"snapshot".to_vec(),
        catalog(selector, selector, slots, None)?,
    );
    Ok(data)
}

pub(super) fn log_state(mut rows: f::Rows, state: RaftLogState) -> f::Rows {
    let anchored = state.last_purged().is_some();
    let mut bytes = if anchored {
        b"FWLS\0\x02".to_vec()
    } else {
        b"FWLS\0\x01".to_vec()
    };
    bytes.push(
        u8::from(state.vote().is_some())
            | u8::from(state.committed().is_some()) << 1
            | u8::from(state.last_log().is_some()) << 2
            | u8::from(anchored) << 3,
    );
    if let Some(vote) = state.vote() {
        bytes.extend_from_slice(&vote.term().to_be_bytes());
        bytes.extend_from_slice(&vote.node().to_be_bytes());
        bytes.push(u8::from(vote.committed()));
    }
    for id in [state.committed(), state.last_log()].into_iter().flatten() {
        put_id(&mut bytes, id);
    }
    if let Some((position, id)) = state.last_purged().zip(state.purged_snapshot_id()) {
        put_id(&mut bytes, position);
        bytes.extend_from_slice(&id);
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-raft-log-v1\0\x01g\0\x01z");
    hash.update(&bytes);
    bytes.extend_from_slice(&hash.finalize());
    assert!(rows.insert(b"state".to_vec(), bytes).is_some());
    rows
}

pub(super) fn anchored(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, [u8; 32])> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 0, blank.as_bytes())?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    log.save_committed(f::at(0))?;
    let wire = artifact(&mut state, true)?;
    let id = facts(&wire)?.0;
    state.pin_snapshot(id, SnapshotLimits::default())?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    // Real independent local proof is the actual committed blank0, still present.
    log.purge_through(f::at(0), id)?;
    Ok((log, state, id))
}

pub(super) fn candidate(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, [u8; 32])> {
    let (mut log, mut state, _) = anchored(dir)?;
    let member = f::member(EntryStamp::Stamped(f::clock(10, 0, 10)?), 4)?;
    f::append(&mut log, 1, member.as_bytes())?;
    f::apply_control(&mut state, 1, member.as_bytes())?;
    log.save_committed(f::at(1))?;
    let wire = artifact(&mut state, true)?;
    Ok((log, state, facts(&wire)?.0))
}

pub(super) fn source(dir: &Path) -> TestResult<Vec<u8>> {
    let (mut log, mut state) = f::create_pair(dir)?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    for index in 0..=4 {
        let (profile, frame) = source_frame(index)?;
        f::append(&mut log, index, frame.as_bytes())?;
        if profile == 0 {
            f::apply_control(&mut state, index, frame.as_bytes())?;
        } else {
            let _ = f::apply_normal(&mut state, index, frame.as_bytes(), profile)?;
        }
    }
    log.save_committed(f::at(4))?;
    artifact(&mut state, true)
}

pub(super) fn source_frame(index: u64) -> TestResult<(u16, wf_ports::log::PreparedBytes)> {
    Ok(match index {
        0 => (0, f::blank(EntryStamp::Unstamped)?),
        1 => (
            5,
            f::frame(5, b"shared", 10, vec![(b"old", f::create_op())])?,
        ),
        2 => (
            1,
            f::frame(1, b"cancel", 11, vec![(b"old", f::cancel_op())])?,
        ),
        3 => (6, f::frame(6, b"new", 12, vec![(b"new", f::create_op())])?),
        4 => (0, f::member(EntryStamp::Stamped(f::clock(20, 0, 20)?), 4)?),
        _ => return Err("unknown source entry".into()),
    })
}

pub(super) fn received(
    mut data: [f::Rows; 6],
    source: &[f::Rows; 6],
    target: usize,
    transition: Option<SnapshotTransition>,
) -> TestResult<[f::Rows; 6]> {
    assert_eq!(source[0][b"snapshot".as_slice()][6], 1);
    let anchor = data[0][b"snapshot".as_slice()][7];
    data[0] = source[0].clone();
    data[1] = source[1].clone();
    data[2 + 2 * target] = source[2].clone();
    data[3 + 2 * target] = source[3].clone();
    if transition.is_none() {
        return finalized(data, target);
    }
    let slots = [slot_facts(&data, 0)?, slot_facts(&data, 1)?];
    data[0].insert(
        b"snapshot".to_vec(),
        catalog(u8::try_from(target + 1)?, anchor, slots, transition)?,
    );
    Ok(data)
}

fn replace_cohort(dir: &Path, data: &[f::Rows; 6]) -> TestResult {
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let mut tx = database.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    for (name, rows) in TABLES.into_iter().zip(data) {
        let definition = TableDefinition::<&[u8], &[u8]>::new(name);
        assert!(tx.delete_table(definition)?);
        let mut table = tx.open_table(definition)?;
        for (key, value) in rows {
            assert!(table.insert(key.as_slice(), value.as_slice())?.is_none());
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn retained_floor(dir: &Path, case: &str) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    let initial = f::blank(EntryStamp::Stamped(f::clock(100, 7, 90)?))?;
    f::append(&mut log, 0, initial.as_bytes())?;
    let mut original = f::apply_control(&mut state, 0, initial.as_bytes())?;
    let p = u64::from(case == "older");
    if p == 1 {
        let blank = f::blank(EntryStamp::Unstamped)?;
        f::append(&mut log, p, blank.as_bytes())?;
        original = f::apply_control(&mut state, p, blank.as_bytes())?;
    }
    log.save_committed(f::at(p))?;
    let old_wire = artifact(&mut state, true)?;
    let old = facts(&old_wire)?;
    state.pin_snapshot(old.0, SnapshotLimits::default())?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    log.purge_through(f::at(p), old.0)?;
    for (index, stamp) in [
        (p + 1, EntryStamp::Unstamped),
        (p + 2, EntryStamp::Stamped(f::clock(200, 0, 200)?)),
    ] {
        let blank = f::blank(stamp)?;
        f::append(&mut log, index, blank.as_bytes())?;
        f::apply_control(&mut state, index, blank.as_bytes())?;
    }
    log.save_committed(f::at(p + 2))?;
    let current = f::applied(&mut state)?.0;
    assert_eq!(
        current.last_entry().map(EntryIdentity::log),
        Some(f::at(p + 2))
    );
    assert_eq!(current.time(), f::clock(200, 0, 200)?);
    drop((log, state));
    let producer = f::directory("independent-p-candidate")?;
    let wire = floor_candidate(&producer, case, original)?;
    let candidate = facts(&wire)?;
    assert_ne!(candidate.0, old.0);
    let source = cohort(&producer)?;
    let mut data = cohort(dir)?;
    assert_eq!(data[0][b"snapshot".as_slice()][6..8], [1, 1]);
    data[4] = source[2].clone();
    data[5] = source[3].clone();
    data[0].insert(
        b"snapshot".to_vec(),
        catalog(2, 1, [Some(old), Some(candidate)], None)?,
    );
    replace_cohort(dir, &data)?;
    let (mut log, mut state) =
        f::open_pair(dir).map_err(|e| format!("P-floor fixture strict reopen failed: {e}"))?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(p)));
    assert_eq!(log.state()?.purged_snapshot_id(), Some(old.0));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old.0));
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    assert_eq!(by_id(&mut state, old.0)?, old_wire);
    assert_eq!(artifact(&mut state, false)?, wire);
    assert_eq!(f::applied(&mut state)?.0, current);
    drop((log, state));
    assert_eq!(cohort(dir)?, data);
    std::fs::remove_dir_all(producer)?;
    Ok(())
}

fn floor_candidate(dir: &Path, case: &str, original: ControlState) -> TestResult<Vec<u8>> {
    let (mut log, mut state) = f::create_pair(dir)?;
    if case == "unapplied" {
        state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
        let wire = artifact(&mut state, true)?;
        assert_eq!(f::applied(&mut state)?.0.last_entry(), None);
        drop((log, state));
        drop(f::open_pair(dir)?);
        return Ok(wire);
    }
    if case == "artifact" {
        let normal = f::frame(5, b"alternate", 10, vec![(b"alternate", f::create_op())])?;
        f::append(&mut log, 0, normal.as_bytes())?;
        let _ = f::apply_normal(&mut state, 0, normal.as_bytes(), 5)?;
    } else {
        let clock = match case {
            "hlc" => f::clock(100, 6, 90)?,
            "wall" => f::clock(100, 7, 89)?,
            "control" | "older" => original.time(),
            _ => return Err("unknown retained P fixture".into()),
        };
        let blank = f::blank(EntryStamp::Stamped(clock))?;
        f::append(&mut log, 0, blank.as_bytes())?;
        f::apply_control(&mut state, 0, blank.as_bytes())?;
    }
    if case == "control" || case == "artifact" {
        let control = if case == "control" {
            ControlState::restored(
                f::clock(101, 7, 91)?,
                original.last_entry().ok_or("P absent")?,
                original.last_entry_stamp().ok_or("P stamp absent")?,
            )?
        } else {
            original
        };
        // Individually coherent DATA can differ; only the saved P ID certifies it.
        drop((log, state));
        super::super::fixtures::replace_control(dir, control, false)?;
        (log, state) = f::open_pair(dir)?;
        assert_eq!(f::applied(&mut state)?.0, control);
        assert_eq!(control == original, case == "artifact");
    } else if case == "hlc" || case == "wall" {
        let blank = f::blank(EntryStamp::Unstamped)?;
        f::append(&mut log, 1, blank.as_bytes())?;
        f::apply_control(&mut state, 1, blank.as_bytes())?;
    }
    log.save_committed(f::at(u64::from(case == "hlc" || case == "wall")))?;
    let wire = artifact(&mut state, true)?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    assert_eq!(artifact(&mut state, false)?, wire);
    drop((log, state));
    drop(f::open_pair(dir)?);
    Ok(wire)
}
