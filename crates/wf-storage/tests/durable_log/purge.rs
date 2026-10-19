//! Inclusive, snapshot-bound retention on the real two-table durable log.

use super::{
    TestResult, assert_entries, assert_state, checksum, create, directory, entries,
    expected_cohort, id, open, put_id, raw_cohort,
};
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogState, RaftLogStore, RaftVote};
use wf_storage::log::RedbLogStore;
use wf_types::version::LogIdentity;

mod corruption;
mod faults;
mod limits;

const SNAPSHOT: [u8; 32] = [17; 32];
const NO_VOTE: &str = "46574c5300020e000000000000000100000000000000070000000000000002000000000000000100000000000000070000000000000002000000000000000100000000000000070000000000000001111111111111111111111111111111111111111111111111111111111111111136fee6ab66ec37e79c13dc432cebe7f025d9d4af7f039bfdd692a5f2f5c159ee";
const WITH_VOTE: &str = "46574c5300020f000000000000000200000000000000000100000000000000010000000000000007000000000000000200000000000000010000000000000007000000000000000200000000000000010000000000000007000000000000000111111111111111111111111111111111111111111111111111111111111111112020b71fbb2d7f302bfa99a0cec4f30860ba8c1bfc4f5605d192464b483b93c2";

fn hex(value: &str) -> TestResult<Vec<u8>> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn anchor_wire(
    vote: Option<(u64, u64, bool)>,
    committed: LogIdentity,
    last: LogIdentity,
    anchor: LogIdentity,
) -> TestResult<Vec<u8>> {
    let mut wire = b"FWLS\0\x02".to_vec();
    wire.push(0b1110 | u8::from(vote.is_some()));
    if let Some((term, node, committed)) = vote {
        wire.extend_from_slice(&term.to_be_bytes());
        wire.extend_from_slice(&node.to_be_bytes());
        wire.push(u8::from(committed));
    }
    for identity in [committed, last, anchor] {
        put_id(&mut wire, identity);
    }
    wire.extend_from_slice(&SNAPSHOT);
    let digest = checksum(b"g", b"z", &wire)?;
    wire.extend_from_slice(&digest);
    Ok(wire)
}

fn anchored(
    specs: &[(LogIdentity, &[u8])],
    vote: Option<(u64, u64, bool)>,
    committed: LogIdentity,
    anchor: LogIdentity,
) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut rows = expected_cohort(b"g", b"z", specs, vote, Some(committed))?;
    let state = rows
        .iter_mut()
        .find(|(key, _)| key == b"Mstate")
        .ok_or("missing fixture state")?;
    state.1 = anchor_wire(
        vote,
        committed,
        specs.last().map_or(anchor, |x| x.0),
        anchor,
    )?;
    Ok(rows)
}

fn assert_anchor(store: &mut RedbLogStore, anchor: LogIdentity) -> TestResult {
    let actual = store.state()?;
    assert_eq!(actual.last_purged(), Some(anchor));
    assert_eq!(actual.purged_snapshot_id(), Some(SNAPSHOT));
    Ok(())
}

#[test]
fn inclusive_purge_preserves_exact_wire_vote_and_retained_rows() -> TestResult {
    let dir = directory("purge-golden")?;
    let path = dir.join("log.redb");
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ];
    let boundary = id(1, 7, 1);
    assert_eq!(
        anchor_wire(None, id(1, 7, 2), id(1, 7, 2), boundary)?,
        hex(NO_VOTE)?
    );
    assert_eq!(
        anchor_wire(Some((2, 0, true)), id(1, 7, 2), id(1, 7, 2), boundary)?,
        hex(WITH_VOTE)?
    );
    let mut store = create(&path, RaftLogLimits::default())?;
    assert_eq!(store.state()?.last_purged(), None);
    assert_eq!(store.state()?.purged_snapshot_id(), None);
    store.append(entries(&specs))?;
    store.save_committed(boundary)?;
    assert!(matches!(
        store.purge_through(id(1, 7, 2), SNAPSHOT),
        Err(RaftLogError::Conflict)
    ));
    assert_eq!(store.state()?.last_purged(), None);
    assert_entries(&mut store, &specs)?;
    store.save_committed(id(1, 7, 2))?;
    store.purge_through(boundary, SNAPSHOT)?;
    assert_anchor(&mut store, boundary)?;
    assert_eq!(store.state()?.last_log(), Some(id(1, 7, 2)));
    assert!(store.read_entry(0, 1024)?.is_none());
    assert!(store.read_entry(1, 1024)?.is_none());
    assert_entries(&mut store, &specs[2..])?;
    drop(store);
    let expected = anchored(&specs[2..], None, id(1, 7, 2), boundary)?;
    assert_eq!(raw_cohort(&path)?, expected);
    let actual = expected
        .iter()
        .find(|(k, _)| k == b"Mstate")
        .ok_or("state absent")?;
    assert_eq!(actual.1, hex(NO_VOTE)?);
    let mut store = open(&path, RaftLogLimits::default())?;
    store.purge_through(boundary, SNAPSHOT)?;
    store.save_vote(RaftVote::new(2, 0, true))?;
    store.save_committed(id(1, 7, 2))?;
    assert_anchor(&mut store, boundary)?;
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        anchored(&specs[2..], Some((2, 0, true)), id(1, 7, 2), boundary)?
    );
    let expected = raw_cohort(&path)?;
    assert_eq!(
        expected
            .iter()
            .find(|(k, _)| k == b"Mstate")
            .ok_or("state absent")?
            .1,
        hex(WITH_VOTE)?
    );
    drop(open(&path, RaftLogLimits::default())?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn empty_physical_log_keeps_anchor_through_append_truncate_and_conflicts() -> TestResult {
    let dir = directory("purge-logical-tail")?;
    let path = dir.join("log.redb");
    let p = id(1, 7, 1);
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&[(id(1, 7, 0), b"zero"), (p, b"one")]))?;
    store.save_committed(p)?;
    store.purge_through(p, SNAPSHOT)?;
    assert_entries(&mut store, &[])?;
    assert_eq!(store.state()?.last_log(), Some(p));
    store.save_committed(p)?;
    for (boundary, snapshot) in [
        (id(1, 7, 0), SNAPSHOT),
        (id(1, 8, 1), SNAPSHOT),
        (p, [18; 32]),
        (id(1, 7, 2), SNAPSHOT),
    ] {
        assert!(matches!(
            store.purge_through(boundary, snapshot),
            Err(RaftLogError::Conflict)
        ));
    }
    for proposed in [
        vec![(p, b"resurrect".as_slice())],
        vec![(id(1, 7, 3), b"gap")],
        vec![(id(1, 6, 2), b"leader-regression")],
    ] {
        assert!(matches!(
            store.append(entries(&proposed)),
            Err(RaftLogError::Conflict)
        ));
    }
    assert!(matches!(
        store.truncate_from(1),
        Err(RaftLogError::Conflict)
    ));
    let extension = [(id(2, 0, 2), b"two".as_slice()), (id(2, 0, 3), b"three")];
    store.append(entries(&extension))?;
    assert_anchor(&mut store, p)?;
    store.truncate_from(2)?;
    assert_anchor(&mut store, p)?;
    assert_state(&mut store, None, Some(p), Some(p))?;
    drop(store);
    assert_eq!(raw_cohort(&path)?, anchored(&[], None, p, p)?);
    let mut store = open(&path, RaftLogLimits::default())?;
    store.append(entries(&extension[..1]))?;
    store.save_committed(extension[0].0)?;
    assert_anchor(&mut store, p)?;
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        anchored(&extension[..1], None, extension[0].0, p)?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn explicit_anchor_observations_do_not_change_the_old_constructor() {
    let p = id(1, 7, 1);
    let old = RaftLogState::new(None, Some(p), Some(p));
    assert_eq!(old.last_purged(), None);
    assert_eq!(old.purged_snapshot_id(), None);
    let supplied = RaftLogState::new_with_anchor(None, Some(p), Some(p), Some((p, SNAPSHOT)));
    assert_eq!(supplied.last_purged(), Some(p));
    assert_eq!(supplied.purged_snapshot_id(), Some(SNAPSHOT));
    assert_eq!(supplied.last_log(), Some(p));
}
