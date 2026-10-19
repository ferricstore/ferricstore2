//! Real product log storage: independent framing, durable cohorts and recovery.

#![forbid(unsafe_code)]

use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use sha2::{Digest, Sha256};
use wf_ports::log::{RaftLogEntry, RaftLogError, RaftLogLimits, RaftLogStore, RaftVote};
use wf_storage::log::RedbLogStore;
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{AppliedPosition, LogIdentity};

mod faults;
mod limits;
mod point;
mod recovery;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const MIB: usize = 1_048_576;
const META: TableDefinition<'static, &[u8], &[u8]> = TableDefinition::new("wf_raft_meta_v1");
const LOGS: TableDefinition<'static, u64, &[u8]> = TableDefinition::new("wf_raft_log_v1");
static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

fn directory(tag: &str) -> TestResult<PathBuf> {
    let suffix = NEXT_PATH.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "wf-durable-log-{}-{suffix}-{tag}",
        std::process::id()
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

fn id(term: u64, node: u64, index: u64) -> LogIdentity {
    LogIdentity::new(term, node, AppliedPosition::new(index))
}

fn create(path: &Path, profile: RaftLogLimits) -> TestResult<RedbLogStore> {
    Ok(RedbLogStore::create(
        path,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
        profile,
    )?)
}

fn open(path: &Path, profile: RaftLogLimits) -> TestResult<RedbLogStore> {
    Ok(RedbLogStore::open(
        path,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
        profile,
    )?)
}

fn entries(specs: &[(LogIdentity, &[u8])]) -> Vec<RaftLogEntry> {
    specs
        .iter()
        .map(|(identity, body)| RaftLogEntry::new(*identity, body.to_vec()))
        .collect()
}

fn put_id(output: &mut Vec<u8>, identity: LogIdentity) {
    output.extend_from_slice(&identity.term().to_be_bytes());
    output.extend_from_slice(&identity.leader_node().to_be_bytes());
    output.extend_from_slice(&identity.index().get().to_be_bytes());
}

fn checksum(group: &[u8], recovery: &[u8], prefix: &[u8]) -> TestResult<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(b"wf-raft-log-v1");
    hash.update(u16::try_from(group.len())?.to_be_bytes());
    hash.update(group);
    hash.update(u16::try_from(recovery.len())?.to_be_bytes());
    hash.update(recovery);
    hash.update(prefix);
    Ok(hash.finalize().into())
}

fn manifest(group: &[u8], recovery: &[u8]) -> TestResult<Vec<u8>> {
    let mut wire = b"FWRM\0\x01".to_vec();
    wire.extend_from_slice(&u16::try_from(group.len())?.to_be_bytes());
    wire.extend_from_slice(group);
    wire.extend_from_slice(&u16::try_from(recovery.len())?.to_be_bytes());
    wire.extend_from_slice(recovery);
    let digest = Sha256::digest(&wire);
    wire.extend_from_slice(&digest);
    Ok(wire)
}

fn state_wire(
    group: &[u8],
    recovery: &[u8],
    vote: Option<(u64, u64, bool)>,
    committed: Option<LogIdentity>,
    last: Option<LogIdentity>,
) -> TestResult<Vec<u8>> {
    let mut wire = b"FWLS\0\x01".to_vec();
    wire.push(
        u8::from(vote.is_some())
            | (u8::from(committed.is_some()) << 1)
            | (u8::from(last.is_some()) << 2),
    );
    if let Some((term, node, committed)) = vote {
        wire.extend_from_slice(&term.to_be_bytes());
        wire.extend_from_slice(&node.to_be_bytes());
        wire.push(u8::from(committed));
    }
    if let Some(identity) = committed {
        put_id(&mut wire, identity);
    }
    if let Some(identity) = last {
        put_id(&mut wire, identity);
    }
    let digest = checksum(group, recovery, &wire)?;
    wire.extend_from_slice(&digest);
    Ok(wire)
}

fn entry_wire(
    group: &[u8],
    recovery: &[u8],
    identity: LogIdentity,
    body: &[u8],
) -> TestResult<Vec<u8>> {
    let mut wire = b"FWRL\0\x01".to_vec();
    put_id(&mut wire, identity);
    wire.extend_from_slice(&u64::try_from(body.len())?.to_be_bytes());
    wire.extend_from_slice(body);
    let digest = checksum(group, recovery, &wire)?;
    wire.extend_from_slice(&digest);
    Ok(wire)
}

fn expected_cohort(
    group: &[u8],
    recovery: &[u8],
    specs: &[(LogIdentity, &[u8])],
    vote: Option<(u64, u64, bool)>,
    committed: Option<LogIdentity>,
) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut rows = vec![
        (b"Mmanifest".to_vec(), manifest(group, recovery)?),
        (
            b"Mstate".to_vec(),
            state_wire(
                group,
                recovery,
                vote,
                committed,
                specs.last().map(|(identity, _)| *identity),
            )?,
        ),
    ];
    for (identity, body) in specs {
        let mut key = vec![b'L'];
        key.extend_from_slice(&identity.index().get().to_be_bytes());
        rows.push((key, entry_wire(group, recovery, *identity, body)?));
    }
    rows.sort();
    Ok(rows)
}

fn raw_cohort(path: &Path) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let database = redb::Builder::new().set_cache_size(16 * MIB).open(path)?;
    let tx = database.begin_read()?;
    let mut rows = Vec::new();
    for row in tx.open_table(META)?.iter()? {
        let (key, value) = row?;
        let mut tagged = vec![b'M'];
        tagged.extend_from_slice(key.value());
        rows.push((tagged, value.value().to_vec()));
    }
    for row in tx.open_table(LOGS)?.iter()? {
        let (key, value) = row?;
        let mut tagged = vec![b'L'];
        tagged.extend_from_slice(&key.value().to_be_bytes());
        rows.push((tagged, value.value().to_vec()));
    }
    rows.sort();
    Ok(rows)
}

fn assert_state(
    store: &mut RedbLogStore,
    vote: Option<(u64, u64, bool)>,
    committed: Option<LogIdentity>,
    last: Option<LogIdentity>,
) -> TestResult {
    let actual = store.state()?;
    assert_eq!(
        actual
            .vote()
            .map(|value| (value.term(), value.node(), value.committed())),
        vote
    );
    assert_eq!(actual.committed(), committed);
    assert_eq!(actual.last_log(), last);
    Ok(())
}

fn assert_entries(store: &mut RedbLogStore, specs: &[(LogIdentity, &[u8])]) -> TestResult {
    let actual = store.read_page(Bound::Unbounded, Bound::Unbounded)?;
    assert_eq!(actual.len(), specs.len());
    for (entry, (identity, body)) in actual.iter().zip(specs) {
        assert_eq!(entry.id(), *identity);
        assert_eq!(entry.body(), *body);
    }
    Ok(())
}

fn refused_append(
    path: &Path,
    specs: &[(LogIdentity, &[u8])],
    proposed: &[(LogIdentity, &[u8])],
) -> TestResult {
    let expected = expected_cohort(b"g", b"z", specs, None, None)?;
    let mut store = open(path, RaftLogLimits::default())?;
    assert!(matches!(
        store.append(entries(proposed)),
        Err(RaftLogError::Conflict)
    ));
    assert_entries(&mut store, specs)?;
    drop(store);
    assert_eq!(raw_cohort(path)?, expected);
    Ok(())
}

#[test]
fn log_cross_call_and_reopen() -> TestResult {
    let dir = directory("cross")?;
    let path = dir.join("log.redb");
    let specs = [
        (id(1, 7, 0), b"a\0\xff".as_slice()),
        (id(1, 7, 1), b"b"),
        (id(1, 9, 2), b"c"),
        (id(2, 1, 3), b"d"),
    ];
    let mut store = create(&path, RaftLogLimits::default())?;
    assert_eq!(store.group().as_bytes(), b"g");
    assert_eq!(store.recovery().as_bytes(), b"z");
    assert_state(&mut store, None, None, None)?;
    store.append(entries(&specs[..1]))?;
    store.append(entries(&specs[1..3]))?;
    store.append(entries(&specs[1..]))?;
    store.append(entries(&specs))?;
    store.append(Vec::new())?;
    assert_state(&mut store, None, None, Some(id(2, 1, 3)))?;
    assert_entries(&mut store, &specs)?;
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &specs, None, None)?
    );
    for proposed in [
        vec![(id(2, 1, 5), b"gap".as_slice())],
        vec![(id(1, 7, 1), b"wrong")],
        vec![(id(1, 8, 1), b"b")],
        vec![(id(2, 0, 4), b"node")],
        vec![(id(1, 99, 4), b"term")],
        vec![(id(2, 2, 4), b"e"), (id(2, 1, 5), b"f")],
    ] {
        refused_append(&path, &specs, &proposed)?;
    }
    let mut reopened = open(&path, RaftLogLimits::default())?;
    assert_entries(&mut reopened, &specs)?;
    drop(reopened);
    let fresh_path = dir.join("fresh.redb");
    let mut fresh = create(&fresh_path, RaftLogLimits::default())?;
    assert!(matches!(
        fresh.append(entries(&[(id(1, 1, 1), b"gap")])),
        Err(RaftLogError::Conflict)
    ));
    drop(fresh);
    assert_eq!(
        raw_cohort(&fresh_path)?,
        expected_cohort(b"g", b"z", &[], None, None)?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn vote_checks(store: &mut RedbLogStore) -> TestResult {
    for vote in [(1, 7, false), (1, 7, true), (1, 8, false), (2, 0, false)] {
        store.save_vote(RaftVote::new(vote.0, vote.1, vote.2))?;
    }
    store.save_vote(RaftVote::new(2, 0, false))?;
    for vote in [(1, 99, true), (1, 8, false)] {
        assert!(matches!(
            store.save_vote(RaftVote::new(vote.0, vote.1, vote.2)),
            Err(RaftLogError::Conflict)
        ));
    }
    store.save_vote(RaftVote::new(2, 0, true))?;
    assert!(matches!(
        store.save_vote(RaftVote::new(2, 0, false)),
        Err(RaftLogError::Conflict)
    ));
    Ok(())
}

#[test]
fn log_vote_and_committed_prefix() -> TestResult {
    let dir = directory("commit")?;
    let path = dir.join("log.redb");
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(2, 0, 2), b"two"),
        (id(2, 0, 3), b"three"),
    ];
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&specs))?;
    vote_checks(&mut store)?;
    store.save_committed(id(1, 7, 0))?;
    store.save_committed(id(1, 7, 1))?;
    store.save_committed(id(1, 7, 1))?;
    for identity in [id(1, 7, 0), id(1, 8, 1), id(2, 0, 4)] {
        assert!(matches!(
            store.save_committed(identity),
            Err(RaftLogError::Conflict)
        ));
    }
    for index in [0, 1] {
        assert!(matches!(
            store.truncate_from(index),
            Err(RaftLogError::Conflict)
        ));
    }
    store.truncate_from(u64::MAX)?;
    assert_entries(&mut store, &specs)?;
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &specs, Some((2, 0, true)), Some(id(1, 7, 1)))?
    );
    let mut store = open(&path, RaftLogLimits::default())?;
    store.truncate_from(2)?;
    let replacement = [(id(3, 0, 2), b"replacement".as_slice())];
    store.append(entries(&replacement))?;
    store.save_committed(id(3, 0, 2))?;
    assert_state(
        &mut store,
        Some((2, 0, true)),
        Some(id(3, 0, 2)),
        Some(id(3, 0, 2)),
    )?;
    drop(store);
    let finals = [specs[0], specs[1], replacement[0]];
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &finals, Some((2, 0, true)), Some(id(3, 0, 2)))?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
