//! Accepted received-snapshot component inputs, never a fabricated Raft issuer.

use super::{
    Error, RaftLogStore, SnapshotLimits, TestResult, caps, f, fixtures, recover_snapshotted,
};
use std::{io::Cursor, path::Path};
use wf_domain::workflow::control::EntryIdentity;
use wf_node::recovery::{install_received_snapshot, restore_received_snapshot};
use wf_ports::{
    log::{RaftLogEntry, RaftLogState, RaftVote},
    snapshot::{SnapshotTransition, StateSnapshotTransitionStore},
};
use wf_storage::{
    log::RedbLogStore,
    snapshot::RedbSnapshotStage,
    state::{RedbStateStore, StateStoreLimits},
};
use wf_types::version::{AppliedPosition, LogIdentity};

pub(super) fn stage(dir: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let scope = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        &dir.join("candidate.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}

fn receiver(dir: &Path, flavor: u8) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let (mut log, mut state, _) = fixtures::anchored(dir)?;
    let (_, shared) = fixtures::source_frame(1)?;
    f::append(&mut log, 1, shared.as_bytes())?;
    let _ = f::apply_normal(&mut state, 1, shared.as_bytes(), 5)?;
    log.save_committed(f::at(1))?;
    log.save_vote(RaftVote::new(3, 9, true))?;
    if flavor != 2 {
        for index in 2..=4 {
            let (_, source_frame) = fixtures::source_frame(index)?;
            let frame = if flavor == 3 && index == 4 {
                f::blank(wf_domain::workflow::control::EntryStamp::Unstamped)?
            } else {
                source_frame
            };
            let id = if flavor == 1 && index == 4 {
                LogIdentity::new(2, 8, AppliedPosition::new(index))
            } else {
                f::at(index)
            };
            log.append(vec![RaftLogEntry::new(id, frame.as_bytes().to_vec())])?;
        }
        let id = if flavor == 1 {
            LogIdentity::new(2, 8, AppliedPosition::new(5))
        } else {
            f::at(5)
        };
        log.append(vec![RaftLogEntry::new(
            id,
            b"uncommitted opaque tail".to_vec(),
        )])?;
    }
    Ok((log, state))
}

#[test]
fn equal_full_received_identity_with_different_actual_body_refuses_before_state_publication()
-> TestResult {
    let source_dir = f::directory("received-body-source")?;
    let wire = fixtures::source(&source_dir)?;
    let dir = f::directory("received-body-target")?;
    let (log, state) = receiver(&dir, 3)?;
    drop((log, state));
    let before = fixtures::cohort(&dir)?;
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    let candidate = stage(&dir, &wire)?;
    let (log, state) = f::open_pair(&dir)?;
    let scope = f::scope()?;
    assert_eq!(
        install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default()
        )
        .err()
        .ok_or("wrong same-ID body accepted")?,
        Error::Recovery(super::CommittedRecoveryError::BodyMismatch)
    );
    drop(f::open_pair(&dir)?);
    assert_eq!(fixtures::cohort(&dir)?, before);
    assert_eq!(f::log_meta(&dir)?, meta);
    assert_eq!(f::log_rows(&dir)?, rows);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(source_dir)?;
    Ok(())
}

#[test]
fn real_received_histories_match_conflict_or_advance_beyond_tail_with_exact_new_authority()
-> TestResult {
    let source_dir = f::directory("received-real-source")?;
    let wire = fixtures::source(&source_dir)?;
    let source = fixtures::cohort(&source_dir)?;
    let id = fixtures::facts(&wire)?.0;
    for flavor in 0..3 {
        let dir = f::directory("received-real-target")?;
        let (mut log, state) = receiver(&dir, flavor)?;
        let old = log.state()?;
        drop((log, state));
        let before = fixtures::cohort(&dir)?;
        let expected = fixtures::received(before.clone(), &source, 1, None)?;
        let mut rows = f::log_rows(&dir)?;
        rows.retain(|index, _| flavor == 0 && *index > 4);
        let expected_meta = fixtures::log_state(
            f::log_meta(&dir)?,
            RaftLogState::new_with_anchor(
                old.vote(),
                Some(f::at(4)),
                Some(if flavor == 0 { f::at(5) } else { f::at(4) }),
                Some((f::at(4), id)),
            ),
        );
        let candidate = stage(&dir, &wire)?;
        let (log, state) = f::open_pair(&dir)?;
        let scope = f::scope()?;
        // The fixture supplies a trusted decision; it does not create a quorum/leader callback.
        let (mut log, mut state) = install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(4)));
        assert_eq!(log.state()?.committed(), Some(f::at(4)));
        assert_eq!(log.state()?.vote(), old.vote());
        assert_eq!(fixtures::by_id(&mut state, id)?, wire);
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        let (control, member) = f::applied(&mut state)?;
        assert_eq!(control.last_entry().map(EntryIdentity::log), Some(f::at(4)));
        assert_eq!(member.entry().map(EntryIdentity::log), Some(f::at(4)));
        assert_eq!(member.membership().nodes()[3], (4, "β".into()));
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_ne!(expected[1], before[1]);
        assert!(
            before[1]
                .keys()
                .any(|key| key.get(6) == Some(&4) && !expected[1].contains_key(key))
        );
        assert_eq!(f::log_rows(&dir)?, rows);
        assert_eq!(f::log_meta(&dir)?, expected_meta);
        std::fs::create_dir(dir.join("retry"))?;
        let candidate = stage(&dir.join("retry"), &wire)?;
        let (log, state) = f::open_pair(&dir)?;
        drop(install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?);
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!(f::log_meta(&dir)?, expected_meta);
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(source_dir)?;
    Ok(())
}

#[test]
fn authorized_received_pending_old_or_new_log_boundary_resumes_forward_atomically() -> TestResult {
    let source_dir = f::directory("received-pending-source")?;
    let wire = fixtures::source(&source_dir)?;
    let source = fixtures::cohort(&source_dir)?;
    let id = fixtures::facts(&wire)?.0;
    for published in [false, true] {
        let dir = f::directory("received-pending-target")?;
        let (mut log, state) = receiver(&dir, 2)?;
        let old = log.state()?;
        let transition = SnapshotTransition::received(old, f::at(4), id)?;
        drop((log, state));
        let before = fixtures::cohort(&dir)?;
        let pending = fixtures::received(before, &source, 1, Some(transition))?;
        let expected = fixtures::finalized(pending.clone(), 1)?;
        let expected_meta = fixtures::log_state(
            f::log_meta(&dir)?,
            RaftLogState::new_with_anchor(
                old.vote(),
                Some(f::at(4)),
                Some(f::at(4)),
                Some((f::at(4), id)),
            ),
        );
        let candidate = stage(&dir, &wire)?;
        let (mut log, mut state) = f::open_pair(&dir)?;
        state.prepare_received_snapshot(candidate, transition, SnapshotLimits::default())?;
        assert_eq!(
            state.snapshot_transition(SnapshotLimits::default())?,
            Some(transition)
        );
        let control = f::applied(&mut state)?.0;
        assert_eq!(control.last_entry().map(EntryIdentity::log), Some(f::at(4)));
        if published {
            log.install_snapshot_boundary(
                old,
                f::at(4),
                *control
                    .last_entry()
                    .ok_or("no received S")?
                    .prepared()
                    .as_bytes(),
                id,
            )?;
        }
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, pending);
        let scope = f::scope()?;
        let (log, state) = f::open_pair(&dir)?;
        let (mut log, mut state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        assert_eq!(log.state()?.committed(), Some(f::at(4)));
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!(f::log_meta(&dir)?, expected_meta);
        assert!(f::log_rows(&dir)?.is_empty());
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(source_dir)?;
    Ok(())
}

#[test]
fn private_fresh_restore_requires_unpurged_log_and_returns_original_data() -> TestResult {
    let source_dir = f::directory("received-fresh-source")?;
    let wire = fixtures::source(&source_dir)?;
    let source = fixtures::cohort(&source_dir)?;
    let id = fixtures::facts(&wire)?.0;
    for purged in [false, true] {
        let dir = f::directory("received-fresh-log")?;
        let (log, state, _) = if purged {
            fixtures::anchored(&dir)?
        } else {
            let (log, state) = f::create_pair(&dir)?;
            (log, state, [0; 32])
        };
        drop(state);
        let expected_log = {
            drop(log);
            (f::log_meta(&dir)?, f::log_rows(&dir)?)
        };
        let expected_state = if purged {
            Some(fixtures::cohort(&dir)?)
        } else {
            None
        };
        let scope = f::scope()?;
        let candidate = stage(&dir, &wire)?;
        let log = RedbLogStore::open(
            &dir.join("log.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            wf_ports::log::RaftLogLimits::default(),
        )?;
        let result = restore_received_snapshot(
            log,
            candidate,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        );
        if purged {
            assert_eq!(
                result.err().ok_or("fresh path invented old DATA")?,
                Error::MissingSnapshot
            );
            assert_eq!(f::log_meta(&dir)?, expected_log.0);
            assert_eq!(f::log_rows(&dir)?, expected_log.1);
            assert_eq!(
                fixtures::cohort(&dir)?,
                expected_state.ok_or("missing old state oracle")?
            );
        } else {
            let (mut log, mut state) = result?;
            assert_eq!(log.state()?.last_purged(), Some(f::at(4)));
            assert_eq!(fixtures::by_id(&mut state, id)?, wire);
            assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
            drop((log, state));
            std::fs::rename(dir.join("state.redb"), dir.join("discarded-initial.redb"))?;
            std::fs::rename(dir.join("candidate.redb"), dir.join("state.redb"))?;
            let mut expected = source.clone();
            expected = fixtures::finalized(expected, 0)?;
            assert_eq!(fixtures::cohort(&dir)?, expected);
            assert_eq!(
                f::log_meta(&dir)?,
                fixtures::log_state(
                    expected_log.0,
                    RaftLogState::new_with_anchor(
                        None,
                        Some(f::at(4)),
                        Some(f::at(4)),
                        Some((f::at(4), id))
                    )
                )
            );
            assert!(f::log_rows(&dir)?.is_empty());
        }
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(source_dir)?;
    Ok(())
}
