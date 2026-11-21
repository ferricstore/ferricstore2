//! The entry log (ADR-018): identified entries, truncations, votes and
//! purges as group log records, replayed on open; appends report durability
//! through their completion; a purge removes whole segments and keeps the
//! vote.

use std::sync::mpsc;
use std::time::Duration;

use wf_storage::log::{EntryId, EntryLog, EntryLogError, GroupLogLimits};

use super::{TestResult, directory};

fn id(term: u64, index: u64) -> EntryId {
    EntryId {
        term,
        node: 1,
        index,
    }
}

fn bytes(index: u64) -> Vec<u8> {
    format!("entry-{index}").into_bytes()
}

/// Appends entries `range` in `term` and waits for their completion.
fn append(log: &EntryLog, term: u64, range: std::ops::Range<u64>) -> TestResult {
    let (sender, durable) = mpsc::channel();
    let entries = range.map(|index| (id(term, index), bytes(index))).collect();
    log.append(
        entries,
        Box::new(move |outcome| {
            let _sent = sender.send(outcome);
        }),
    )?;
    durable.recv_timeout(Duration::from_secs(10))??;
    Ok(())
}

fn kept(log: &EntryLog, range: std::ops::Range<u64>) -> Vec<(EntryId, Vec<u8>)> {
    log.entries(range)
        .into_iter()
        .map(|(id, bytes)| (id, bytes.to_vec()))
        .collect()
}

#[test]
fn entries_votes_and_truncations_survive_a_reopen() -> TestResult {
    let directory = directory("entries")?;
    let path = directory.join("log");
    {
        let log = EntryLog::open(&path, GroupLogLimits::default())?;
        assert_eq!(log.bounds(), (None, None));
        append(&log, 1, 1..6)?;
        log.save_vote(b"vote-2")?;
        // A follower drops an uncommitted suffix and takes the new leader's.
        log.truncate_after(Some(3))?;
        append(&log, 2, 4..6)?;
        assert_eq!(
            kept(&log, 3..5),
            vec![(id(1, 3), bytes(3)), (id(2, 4), bytes(4))]
        );
    }
    let log = EntryLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(log.bounds(), (None, Some(id(2, 5))));
    assert_eq!(log.vote(), Some(b"vote-2".to_vec()));
    let terms: Vec<u64> = kept(&log, 0..100).iter().map(|(id, _)| id.term).collect();
    assert_eq!(terms, vec![1, 1, 1, 2, 2]);
    log.truncate_after(None)?;
    assert_eq!(log.bounds(), (None, None));
    Ok(())
}

#[test]
fn an_entry_that_leaves_a_gap_is_refused() -> TestResult {
    let directory = directory("entries-gap")?;
    let log = EntryLog::open(&directory.join("log"), GroupLogLimits::default())?;
    append(&log, 1, 1..3)?;
    let refused = log.append(vec![(id(1, 5), bytes(5))], Box::new(|_| {}));
    assert_eq!(refused, Err(EntryLogError::Gap { index: 5 }));
    // An overwrite from an index already held replaces the suffix.
    append(&log, 2, 2..3)?;
    assert_eq!(log.bounds(), (None, Some(id(2, 2))));
    Ok(())
}

#[test]
fn a_purge_removes_whole_segments_and_keeps_the_vote() -> TestResult {
    let directory = directory("entries-purge")?;
    let path = directory.join("log");
    let limits = GroupLogLimits {
        max_segment_bytes: 300,
        ..GroupLogLimits::default()
    };
    {
        let log = EntryLog::open(&path, limits)?;
        log.save_vote(b"vote-1")?;
        for index in 1..40 {
            append(&log, 1, index..index + 1)?;
        }
        let before = std::fs::read_dir(&path)?.count();
        log.purge(id(1, 30))?;
        let after = std::fs::read_dir(&path)?.count();
        assert!(after < before, "{before} segments, then {after}");
        assert_eq!(log.bounds(), (Some(id(1, 30)), Some(id(1, 39))));
        assert_eq!(kept(&log, 0..31), Vec::new());
    }
    let log = EntryLog::open(&path, limits)?;
    assert_eq!(log.bounds(), (Some(id(1, 30)), Some(id(1, 39))));
    assert_eq!(log.vote(), Some(b"vote-1".to_vec()));
    assert_eq!(
        kept(&log, 31..33),
        vec![(id(1, 31), bytes(31)), (id(1, 32), bytes(32))]
    );
    // The next entry follows the kept ones; after a purge of everything it
    // follows the purged id.
    log.purge(id(1, 39))?;
    append(&log, 1, 40..41)?;
    assert_eq!(log.bounds(), (Some(id(1, 39)), Some(id(1, 40))));
    Ok(())
}

#[test]
fn a_reopened_log_continues_after_its_last_durable_entry() -> TestResult {
    let directory = directory("entries-reopen")?;
    let path = directory.join("log");
    let log = EntryLog::open(&path, GroupLogLimits::default())?;
    append(&log, 1, 1..2)?;
    drop(log);
    let log = EntryLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(log.bounds(), (None, Some(id(1, 1))));
    let refused = log.append(vec![(id(1, 9), bytes(9))], Box::new(|_| {}));
    assert_eq!(refused, Err(EntryLogError::Gap { index: 9 }));
    append(&log, 1, 2..3)?;
    assert_eq!(log.bounds(), (None, Some(id(1, 2))));
    Ok(())
}
