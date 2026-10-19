//! Trusted received boundaries preserve committed history and atomic log cohorts.

use super::{
    TestResult, assert_entries, checksum, create, directory, entries, expected_cohort, id, open,
    put_id, raw_cohort,
};
use sha2::{Digest, Sha256};
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogState, RaftLogStore, RaftVote};
use wf_storage::log::RedbLogStore;
use wf_types::version::LogIdentity;

mod faults;
mod limits;

const ARTIFACT: [u8; 32] = [29; 32];

fn digest(body: &[u8]) -> [u8; 32] {
    Sha256::digest(body).into()
}

fn expected(
    specs: &[(LogIdentity, &[u8])],
    vote: Option<(u64, u64, bool)>,
    position: LogIdentity,
) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut result = expected_cohort(b"g", b"z", specs, vote, Some(position))?;
    let mut wire = b"FWLS\0\x02".to_vec();
    wire.push(0b1110 | u8::from(vote.is_some()));
    if let Some((term, node, committed)) = vote {
        wire.extend_from_slice(&term.to_be_bytes());
        wire.extend_from_slice(&node.to_be_bytes());
        wire.push(u8::from(committed));
    }
    for value in [position, specs.last().map_or(position, |x| x.0), position] {
        put_id(&mut wire, value);
    }
    wire.extend_from_slice(&ARTIFACT);
    wire.extend_from_slice(&checksum(b"g", b"z", &wire)?);
    result
        .iter_mut()
        .find(|(key, _)| key == b"Mstate")
        .ok_or("missing independently packed state")?
        .1 = wire;
    Ok(result)
}

fn boundary(
    store: &mut RedbLogStore,
    position: LogIdentity,
    tail: LogIdentity,
    vote: Option<RaftVote>,
) -> TestResult {
    assert_eq!(
        store.state()?,
        RaftLogState::new_with_anchor(vote, Some(position), Some(tail), Some((position, ARTIFACT)))
    );
    assert!(store.read_entry(position.index().get(), 1024)?.is_none());
    Ok(())
}

#[test]
fn matching_received_boundary_preserves_suffix_vote_and_exact_retry_after_reopen() -> TestResult {
    let dir = directory("received-match")?;
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
        (id(1, 7, 3), b"three"),
    ];
    for vote in [None, Some((3, 8, true))] {
        let path = dir.join(format!("log-{}.redb", vote.is_some()));
        let mut store = create(&path, RaftLogLimits::default())?;
        store.append(entries(&specs))?;
        if let Some((term, node, committed)) = vote {
            store.save_vote(RaftVote::new(term, node, committed))?;
        }
        store.save_committed(specs[0].0)?;
        let old = store.state()?;
        store.install_snapshot_boundary(old, specs[1].0, digest(b"one"), ARTIFACT)?;
        store.install_snapshot_boundary(old, specs[1].0, digest(b"one"), ARTIFACT)?;
        boundary(&mut store, specs[1].0, specs[3].0, old.vote())?;
        assert_entries(&mut store, &specs[2..])?;
        drop(store);
        let whole = expected(&specs[2..], vote, specs[1].0)?;
        assert_eq!(raw_cohort(&path)?, whole);
        let mut store = open(&path, RaftLogLimits::default())?;
        // The exact-new boundary wins over the now-stale OLD metadata CAS.
        store.install_snapshot_boundary(old, specs[1].0, digest(b"one"), ARTIFACT)?;
        boundary(&mut store, specs[1].0, specs[3].0, old.vote())?;
        assert!(matches!(
            store.install_snapshot_boundary(old, specs[1].0, digest(b"one"), [30; 32]),
            Err(RaftLogError::Conflict)
        ));
        drop(store);
        assert_eq!(raw_cohort(&path)?, whole);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn conflicting_received_boundary_discards_uncommitted_branch_without_fabricating_row() -> TestResult
{
    let dir = directory("received-conflict")?;
    let path = dir.join("log.redb");
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"old-one"),
        (id(1, 7, 2), b"old-two"),
        (id(1, 7, 3), b"old-three"),
    ];
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&specs))?;
    store.save_committed(specs[0].0)?;
    let vote = RaftVote::new(3, 8, true);
    store.save_vote(vote)?;
    let old = store.state()?;
    let received = id(2, 9, 1);
    store.install_snapshot_boundary(old, received, digest(b"received-body"), ARTIFACT)?;
    boundary(&mut store, received, received, Some(vote))?;
    assert_entries(&mut store, &[])?;
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        expected(&[], Some((3, 8, true)), received)?
    );
    let mut store = open(&path, RaftLogLimits::default())?;
    store.install_snapshot_boundary(old, received, digest(b"received-body"), ARTIFACT)?;
    store.append(entries(&[(id(2, 9, 2), b"new-suffix")]))?;
    boundary(&mut store, received, id(2, 9, 2), Some(vote))?;
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        expected(
            &[(id(2, 9, 2), b"new-suffix")],
            Some((3, 8, true)),
            received
        )?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn trusted_beyond_tail_and_max_boundaries_have_only_logical_rows() -> TestResult {
    let dir = directory("received-beyond")?;
    for index in [100, u64::MAX] {
        let path = dir.join(format!("log-{index}.redb"));
        let mut store = create(&path, RaftLogLimits::default())?;
        store.append(entries(&[(id(1, 7, 0), b"zero"), (id(1, 7, 1), b"one")]))?;
        store.save_committed(id(1, 7, 0))?;
        let old = store.state()?;
        let received = id(2, 9, index);
        // This explicitly trusted port test does not claim a Raft authority issuer.
        store.install_snapshot_boundary(old, received, digest(b"remote"), ARTIFACT)?;
        boundary(&mut store, received, received, None)?;
        assert_entries(&mut store, &[])?;
        assert!(matches!(
            store.append(entries(&[(received, b"resurrect")])),
            Err(RaftLogError::Conflict)
        ));
        drop(store);
        let whole = expected(&[], None, received)?;
        assert_eq!(raw_cohort(&path)?, whole);
        let mut store = open(&path, RaftLogLimits::default())?;
        store.install_snapshot_boundary(old, received, digest(b"remote"), ARTIFACT)?;
        boundary(&mut store, received, received, None)?;
        drop(store);
        assert_eq!(raw_cohort(&path)?, whole);
    }
    let path = dir.join("initial-empty.redb");
    let mut store = create(&path, RaftLogLimits::default())?;
    let old = store.state()?;
    let received = id(1, 7, 0);
    store.install_snapshot_boundary(old, received, digest(b"first"), ARTIFACT)?;
    boundary(&mut store, received, received, None)?;
    drop(store);
    assert_eq!(raw_cohort(&path)?, expected(&[], None, received)?);
    drop(open(&path, RaftLogLimits::default())?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn body_commit_leader_and_every_metadata_cas_refusal_preserve_complete_old_cohort() -> TestResult {
    let dir = directory("received-refusals")?;
    for case in 0..9 {
        let path = dir.join(format!("log-{case}.redb"));
        let leader = if case == 3 { (2, 8) } else { (1, 7) };
        let specs = [
            (id(leader.0, leader.1, 0), b"zero".as_slice()),
            (id(leader.0, leader.1, 1), b"one"),
            (id(leader.0, leader.1, 2), b"two"),
            (id(leader.0, leader.1, 3), b"three"),
        ];
        let mut store = create(&path, RaftLogLimits::default())?;
        store.append(entries(&specs))?;
        store.save_committed(specs[0].0)?;
        let mut old = store.state()?;
        let mut target = specs[1].0;
        let mut prepared = digest(b"one");
        match case {
            0 => prepared = [0; 32],
            1 => store.save_vote(RaftVote::new(3, 8, true))?,
            2 => target = specs[0].0,
            3 => target = id(1, 7, 1),
            4 => {
                store.save_committed(specs[1].0)?;
                target = specs[2].0;
                prepared = digest(b"two");
            }
            5 => store.append(entries(&[(id(1, 7, 4), b"four")]))?,
            6 => store.purge_through(specs[0].0, [8; 32])?,
            7 => {
                store.purge_through(specs[0].0, [8; 32])?;
                old = store.state()?;
                old = RaftLogState::new_with_anchor(
                    old.vote(),
                    old.committed(),
                    old.last_log(),
                    Some((specs[0].0, [9; 32])),
                );
            }
            8 => {
                store.save_committed(specs[2].0)?;
                old = store.state()?;
            }
            _ => return Err("unknown refusal case".into()),
        }
        let before = store.state()?;
        drop(store);
        let whole = raw_cohort(&path)?;
        let mut store = open(&path, RaftLogLimits::default())?;
        assert!(
            matches!(
                store.install_snapshot_boundary(old, target, prepared, ARTIFACT),
                Err(RaftLogError::Conflict)
            ),
            "case {case}"
        );
        assert_eq!(store.state()?, before, "healthy refusal fenced case {case}");
        drop(store);
        assert_eq!(raw_cohort(&path)?, whole, "case {case}");
        drop(open(&path, RaftLogLimits::default())?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn received_boundary_advances_a_previous_purge_base_without_resurrecting_it() -> TestResult {
    let dir = directory("received-old-base")?;
    let path = dir.join("log.redb");
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
        (id(1, 7, 3), b"three"),
    ];
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&specs))?;
    store.save_committed(specs[1].0)?;
    store.purge_through(specs[1].0, [8; 32])?;
    let old = store.state()?;
    store.install_snapshot_boundary(old, specs[2].0, digest(b"two"), ARTIFACT)?;
    boundary(&mut store, specs[2].0, specs[3].0, None)?;
    assert_entries(&mut store, &specs[3..])?;
    drop(store);
    assert_eq!(raw_cohort(&path)?, expected(&specs[3..], None, specs[2].0)?);
    drop(open(&path, RaftLogLimits::default())?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
