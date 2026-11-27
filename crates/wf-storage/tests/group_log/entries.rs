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

/// The mark that the member votes (ADR-019) survives a reopen and a purge
/// that removes the segment it was written in; a new log has none.
#[test]
fn the_voter_mark_survives_a_reopen_and_a_purge() -> TestResult {
    let directory = directory("entries-voter")?;
    let path = directory.join("log");
    let limits = GroupLogLimits {
        max_segment_bytes: 300,
        ..GroupLogLimits::default()
    };
    {
        let log = EntryLog::open(&path, limits)?;
        assert!(!log.voter());
        log.save_voter()?;
        log.save_voter()?;
        assert!(log.voter());
        for index in 1..40 {
            append(&log, 1, index..index + 1)?;
        }
        log.purge(id(1, 30))?;
    }
    let log = EntryLog::open(&path, limits)?;
    assert!(log.voter());
    assert_eq!(log.bounds(), (Some(id(1, 30)), Some(id(1, 39))));
    let fresh = EntryLog::open(&directory.join("fresh"), limits)?;
    assert!(!fresh.voter());
    Ok(())
}

/// A closed log (its member stopping) refuses every change and changes
/// nothing more, appends included; reading goes on.
#[test]
fn a_closed_entry_log_refuses_every_change_and_still_answers_reads() -> TestResult {
    let directory = directory("entries-close")?;
    let path = directory.join("log");
    let limits = GroupLogLimits {
        max_segment_bytes: 300,
        ..GroupLogLimits::default()
    };
    {
        let log = EntryLog::open(&path, limits)?;
        for index in 1..10 {
            append(&log, 1, index..index + 1)?;
        }
        log.save_vote(b"vote-1")?;
        log.close();
        assert!(log.save_vote(b"vote-2").is_err());
        assert!(log.save_voter().is_err());
        assert!(log.truncate_after(Some(3)).is_err());
        assert!(log.purge(id(1, 8)).is_err());
        assert!(append(&log, 1, 10..11).is_err());
        assert_eq!(log.bounds(), (None, Some(id(1, 9))));
        assert_eq!(log.vote(), Some(b"vote-1".to_vec()));
        assert_eq!(kept(&log, 1..2), vec![(id(1, 1), bytes(1))]);
    }
    let log = EntryLog::open(&path, limits)?;
    assert_eq!(log.bounds(), (None, Some(id(1, 9))));
    assert_eq!(log.vote(), Some(b"vote-1".to_vec()));
    assert!(!log.voter());
    assert_eq!(kept(&log, 0..100).len(), 9);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

/// A purge removes the segments before its oldest kept entry, which can
/// leave an older purge's mark ahead of entries whose predecessors went
/// with those segments: segment 1 holds 4..=6, the purge to 3 lands at
/// the start of segment 2 ahead of 7 and 8, and the purge to 6 removes
/// segment 1. The log still reopens with every kept entry.
#[test]
fn a_log_reopens_after_a_purge_removed_the_entries_after_an_older_purge() -> TestResult {
    let directory = directory("entries-purged-twice")?;
    let path = directory.join("log");
    // One entry is a 60-byte block: three fit a segment, a fourth or a
    // purge mark after three starts the next.
    let limits = GroupLogLimits {
        max_segment_bytes: 200,
        ..GroupLogLimits::default()
    };
    {
        let log = EntryLog::open(&path, limits)?;
        for index in 1..=6 {
            append(&log, 1, index..index + 1)?;
        }
        log.purge(id(1, 3))?;
        for index in 7..=9 {
            append(&log, 1, index..index + 1)?;
        }
        log.purge(id(1, 6))?;
    }
    let log = EntryLog::open(&path, limits)?;
    assert_eq!(log.bounds(), (Some(id(1, 6)), Some(id(1, 9))));
    assert_eq!(
        kept(&log, 0..100),
        (7..=9)
            .map(|index| (id(1, index), bytes(index)))
            .collect::<Vec<_>>()
    );
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
