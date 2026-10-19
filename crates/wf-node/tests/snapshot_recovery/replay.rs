//! Compare snapshot-based replay with real uninterrupted drivers and replies.

use super::{
    CommittedRecoveryError, CommittedRecoveryLimits, Error, RaftLogStore, SnapshotLimits,
    StateSnapshotStore, TestResult, caps, f, fixtures, purge_snapshot, recover_snapshotted,
};
use std::path::Path;
use wf_domain::workflow::{
    control::{EntryIdentity, EntryStamp},
    request::RetainedStatus,
};
use wf_ports::log::PreparedBytes;
use wf_storage::transaction::RecordBatchTransactionOutcome as Outcome;

fn frames() -> TestResult<Vec<(u16, PreparedBytes)>> {
    let first = || f::frame(5, b"a", 10, vec![(b"i", f::create_op())]);
    Ok(vec![
        (0, f::blank(EntryStamp::Unstamped)?),
        (0, f::member(EntryStamp::Stamped(f::clock(10, 0, 10)?), 4)?),
        (5, first()?),
        (5, first()?),
        (5, f::frame(5, b"a", 10, vec![(b"j", f::create_op())])?),
        (
            6,
            f::frame(
                6,
                b"b",
                11,
                vec![(b"j", f::create_op()), (b"i", f::create_op())],
            )?,
        ),
        (1, f::frame(1, b"c", 12, vec![(b"j", f::cancel_op())])?),
        (5, f::frame(5, b"reject", 13, vec![(b"i", f::create_op())])?),
        (0, f::blank(EntryStamp::Stamped(f::clock(2011, 0, 2011)?))?),
        (5, first()?),
        (0, f::member(EntryStamp::Unstamped, 5)?),
    ])
}

fn classification(index: u64, value: &Outcome) -> TestResult {
    match index {
        2 | 3 => {
            let Outcome::Retained { outcome, .. } = value else {
                return Err("original reply missing".into());
            };
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
            assert_eq!(outcome.original_position(), f::at(2));
            assert_eq!(outcome.original_time().get(), 10);
            assert_eq!(outcome.expires_at().get(), 1010);
            assert_eq!(outcome.response(), [0]);
        }
        4 => assert!(matches!(value, Outcome::IntentConflict(_))),
        5..=7 => {
            let Outcome::Retained { outcome, .. } = value else {
                return Err("fresh reply missing".into());
            };
            assert_eq!(outcome.original_position(), f::at(index));
            assert_eq!(outcome.original_time().get(), index + 6);
            let rejected = RetainedStatus::IndexedRejected(
                wf_domain::workflow::indexed::IndexRejection::Batch(
                    wf_domain::workflow::batch::BatchRejection::Domain(
                        wf_domain::workflow::DomainError::AlreadyExists,
                    ),
                ),
            );
            assert_eq!(
                outcome.status(),
                if index == 7 {
                    rejected
                } else {
                    RetainedStatus::Accepted
                }
            );
            assert_eq!(
                outcome.response(),
                match index {
                    5 => &[2, 0, 0, 0, 2, 0, 1, 0, 1][..],
                    6 => &[0][..],
                    _ => &[1, 0, 1][..],
                }
            );
        }
        9 => assert!(matches!(value, Outcome::Expired(_))),
        _ => return Err("unknown Normal classification".into()),
    }
    Ok(())
}

fn build(
    dir: &Path,
    prefix: usize,
) -> TestResult<(
    wf_storage::log::RedbLogStore,
    wf_storage::state::RedbStateStore,
)> {
    assert!((2..=11).contains(&prefix));
    let (mut log, mut state) = f::create_pair(dir)?;
    for (index, (profile, frame)) in frames()?.iter().enumerate() {
        let i = u64::try_from(index)?;
        f::append(&mut log, i, frame.as_bytes())?;
        if index < prefix {
            if *profile == 0 {
                f::apply_control(&mut state, i, frame.as_bytes())?;
            } else {
                classification(
                    i,
                    &f::apply_normal(&mut state, i, frame.as_bytes(), *profile)?,
                )?;
            }
        }
        if index == 1 {
            fixtures::capture(&mut state)?;
        }
    }
    f::append(&mut log, 11, b"uncommitted opaque poison")?;
    log.save_committed(f::at(10))?;
    Ok((log, state))
}

#[test]
fn snapshot_recovery_replays_all_real_profiles_with_original_replies_and_immutable_data()
-> TestResult {
    let reference = f::directory("snapshot-mixed-reference")?;
    let (log, state) = build(&reference, 11)?;
    let s = f::scope()?;
    let (log, mut state) = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    let final_state = f::applied(&mut state)?;
    assert_eq!(
        final_state.1.entry(),
        Some(EntryIdentity::new(f::at(10), frames()?[10].1.digest()))
    );
    assert_eq!(
        final_state.1.membership().configs(),
        [vec![1, 2], vec![2, 3], vec![1, 2]]
    );
    assert_eq!(final_state.1.membership().nodes()[3], (5, "β".into()));
    let wire = fixtures::artifact(&mut state)?;
    drop((log, state));
    let expected = fixtures::cohort(&reference)?;
    let live = f::state_rows(&reference)?;
    assert_eq!(live, expected[1]);
    for (family, count) in [
        (0, 1),
        (1, 2),
        (2, 4),
        (3, 1),
        (4, 1),
        (5, 3),
        (6, 1),
        (7, 1),
    ] {
        assert_eq!(
            live.keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            count
        );
    }
    let logs = f::log_rows(&reference)?;
    let meta = f::log_meta(&reference)?;
    for prefix in [2, 6, 11] {
        let dir = f::directory("snapshot-mixed")?;
        let (log, state) = build(&dir, prefix)?;
        let (mut log, mut state) = purge_snapshot(
            log,
            state,
            s.group(),
            s.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
        assert_eq!(fixtures::artifact(&mut state)?, wire);
        drop((log, state));
        for _ in 0..2 {
            let (log, state) = f::open_pair(&dir)?;
            let (mut log, mut state) = recover_snapshotted(
                log,
                state,
                s.group(),
                s.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )?;
            let observed = f::applied(&mut state)?;
            assert_eq!(observed.0, final_state.0);
            assert!(observed.1 == final_state.1);
            assert_eq!(log.state()?.committed(), Some(f::at(10)));
            assert_eq!(log.state()?.last_log(), Some(f::at(11)));
            assert_eq!(fixtures::artifact(&mut state)?, wire);
            drop((log, state));
            assert_eq!(fixtures::cohort(&dir)?, expected);
            assert_eq!(f::log_rows(&dir)?, logs);
            assert_eq!(f::log_meta(&dir)?, meta);
        }
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

#[test]
fn unpurged_snapshot_recovery_preserves_full_logs_and_does_not_invent_a_pin() -> TestResult {
    let reference = f::directory("unpurged-reference")?;
    drop(build(&reference, 11)?);
    let expected = fixtures::cohort(&reference)?;
    let logs = f::log_rows(&reference)?;
    let meta = f::log_meta(&reference)?;
    let dir = f::directory("unpurged-snapshot")?;
    let (log, state) = build(&dir, 2)?;
    let s = f::scope()?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), None);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, None);
    drop((log, state));
    assert_eq!(fixtures::cohort(&dir)?, expected);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

#[test]
fn missing_artifact_without_purge_uses_real_legacy_recovery_and_keeps_opaque_tail() -> TestResult {
    let dir = f::directory("snapshot-fallback")?;
    let (mut log, state) = f::create_pair(&dir)?;
    f::append(&mut log, 0, b"uncommitted opaque body")?;
    drop((log, state));
    let before = fixtures::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let meta = f::log_meta(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    let s = f::scope()?;
    drop(recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?);
    assert_eq!(fixtures::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn resume_seed(
    dir: &Path,
) -> TestResult<(
    wf_storage::log::RedbLogStore,
    wf_storage::state::RedbStateStore,
)> {
    let (mut log, state, _) = fixtures::seeded(dir)?;
    let member = f::member(EntryStamp::Stamped(f::clock(11, 0, 11)?), 5)?;
    let normal = f::frame(5, b"after-prefix", 12, vec![(b"i", f::create_op())])?;
    f::append(&mut log, 2, member.as_bytes())?;
    f::append(&mut log, 3, normal.as_bytes())?;
    log.save_committed(f::at(3))?;
    let s = f::scope()?;
    Ok(purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?)
}

fn resume_oracles(dir: &Path) -> TestResult<([f::Rows; 4], [f::Rows; 4])> {
    let (log, mut state) = resume_seed(dir)?;
    let member = f::member(EntryStamp::Stamped(f::clock(11, 0, 11)?), 5)?;
    f::apply_control(&mut state, 2, member.as_bytes())?;
    drop((log, state));
    let prefix = fixtures::cohort(dir)?;
    let (log, mut state) = f::open_pair(dir)?;
    let normal = f::frame(5, b"after-prefix", 12, vec![(b"i", f::create_op())])?;
    let Outcome::Retained { outcome, .. } = f::apply_normal(&mut state, 3, normal.as_bytes(), 5)?
    else {
        return Err("uninterrupted reference outcome missing".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), f::at(3));
    assert_eq!(outcome.original_time().get(), 12);
    assert_eq!(outcome.expires_at().get(), 1012);
    assert_eq!(outcome.response(), [0]);
    drop((log, state));
    Ok((prefix, fixtures::cohort(dir)?))
}

#[test]
fn later_replay_refusal_preserves_completed_prefix_for_fresh_reopen_resume() -> TestResult {
    let reference = f::directory("snapshot-prefix-reference")?;
    let (expected_prefix, expected_final) = resume_oracles(&reference)?;
    let dir = f::directory("snapshot-prefix-resume")?;
    drop(resume_seed(&dir)?);
    let normal = f::frame(5, b"after-prefix", 12, vec![(b"i", f::create_op())])?;
    let s = f::scope()?;
    let before = fixtures::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let meta = f::log_meta(&dir)?;
    let mut driver = f::driver_limits();
    driver.workspace_bytes = 1;
    let cap = CommittedRecoveryLimits::new(
        wf_ports::consensus::RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        driver,
        (1_000, 64 * f::MIB),
        16 * f::MIB,
        512 * f::MIB,
    )?;
    let (log, state) = f::open_pair(&dir)?;
    let error = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        cap,
        SnapshotLimits::default(),
    )
    .err()
    .ok_or("tiny Normal workspace unexpectedly succeeded")?;
    assert!(matches!(
        error,
        Error::Recovery(CommittedRecoveryError::Normal(
            wf_storage::transaction::RecordBatchTransactionError::Apply(
                wf_ports::log::RecordBatchApplyError::Workspace { maximum: 1, .. }
            )
        ))
    ));
    let (log, mut state) = f::open_pair(&dir)?;
    let saved = f::applied(&mut state)?;
    assert_eq!(saved.0.last_entry().map(EntryIdentity::log), Some(f::at(2)));
    assert_eq!(saved.1.entry().map(EntryIdentity::log), Some(f::at(2)));
    drop((log, state));
    let after = fixtures::cohort(&dir)?;
    assert_eq!(after, expected_prefix);
    assert_ne!(after[0], before[0]);
    assert_eq!(after[2..], before[2..]);
    assert_ne!(after[1], before[1]);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(
        f::applied(&mut state)?
            .0
            .last_entry()
            .map(EntryIdentity::log),
        Some(f::at(3))
    );
    let outcome = f::apply_normal(&mut state, 3, normal.as_bytes(), 5)?;
    assert!(matches!(outcome, Outcome::LogReplay(_)));
    drop((log, state));
    drop(f::open_pair(&dir)?);
    assert_eq!(fixtures::cohort(&dir)?, expected_final);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}
