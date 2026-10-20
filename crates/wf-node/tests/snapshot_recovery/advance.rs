//! Repeatable artifact custody, exact pending reconciliation and real workflows.

use super::{
    CommittedRecoveryError, CommittedRecoveryLimits, Error, RaftLogStore, SnapshotError,
    SnapshotLimits, StateSnapshotStore, TestResult, caps, disk_backend, f, purge_snapshot,
    recover_snapshotted,
};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_ports::snapshot::{SnapshotTransition, StateSnapshotTransitionStore};

mod faults;
mod fixtures;
mod limits;
mod received;

#[test]
fn three_real_generations_preserve_old_by_id_until_ack_and_reuse_retired_slots() -> TestResult {
    let dir = f::directory("advance-three-generations")?;
    let (mut log, mut state, mut old_id) = fixtures::anchored(&dir)?;
    let mut old_wire = fixtures::by_id(&mut state, old_id)?;
    let scope = f::scope()?;
    for generation in 1_u64..=3 {
        let name = format!("generation-{generation}");
        let normal = f::frame(
            5,
            name.as_bytes(),
            10 + generation,
            vec![(name.as_bytes(), f::create_op())],
        )?;
        f::append(&mut log, generation, normal.as_bytes())?;
        let outcome = f::apply_normal(&mut state, generation, normal.as_bytes(), 5)?;
        assert!(matches!(
            outcome,
            wf_storage::transaction::RecordBatchTransactionOutcome::Retained { .. }
        ));
        log.save_committed(f::at(generation))?;
        let wire = fixtures::artifact(&mut state, true)?;
        let next_id = fixtures::facts(&wire)?.0;
        assert_ne!(next_id, old_id);
        assert_eq!(fixtures::by_id(&mut state, old_id)?, old_wire);
        assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old_id));
        let before_log = log.state()?;
        drop((log, state));
        let before = fixtures::cohort(&dir)?;
        let target = fixtures::slot(&before, next_id)?;
        let expected = fixtures::finalized(before.clone(), target)?;
        let expected_meta = fixtures::log_state(
            f::log_meta(&dir)?,
            wf_ports::log::RaftLogState::new_with_anchor(
                before_log.vote(),
                Some(f::at(generation)),
                Some(f::at(generation)),
                Some((f::at(generation), next_id)),
            ),
        );
        (log, state) = f::open_pair(&dir)?;
        (log, state) = purge_snapshot(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(generation)));
        assert_eq!(fixtures::by_id(&mut state, next_id)?, wire);
        let mut absent = Vec::new();
        assert!(
            state
                .snapshot_by_id(old_id, &mut absent, SnapshotLimits::default())?
                .is_none()
        );
        assert_eq!(absent, []);
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!(f::log_meta(&dir)?, expected_meta);
        assert!(f::log_rows(&dir)?.is_empty());
        (log, state) = f::open_pair(&dir)?;
        (log, state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        old_id = next_id;
        old_wire = wire;
    }
    let observed = f::applied(&mut state)?;
    assert_eq!(
        observed.0.last_entry().map(EntryIdentity::log),
        Some(f::at(3))
    );
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn pending_old_and_new_log_anchors_resume_without_exposing_or_discarding_data() -> TestResult {
    for published in [false, true] {
        let dir = f::directory("advance-pending-local")?;
        let (mut log, mut state, old_id) = fixtures::anchored(&dir)?;
        let member = f::member(EntryStamp::Stamped(f::clock(10, 0, 10)?), 4)?;
        f::append(&mut log, 1, member.as_bytes())?;
        f::apply_control(&mut state, 1, member.as_bytes())?;
        log.save_committed(f::at(1))?;
        let wire = fixtures::artifact(&mut state, true)?;
        let id = fixtures::facts(&wire)?.0;
        let transition = SnapshotTransition::local(log.state()?, f::at(1), id)?;
        drop((log, state));
        let before = fixtures::cohort(&dir)?;
        let pending = fixtures::pending(before.clone(), transition)?;
        let expected = fixtures::finalized(pending.clone(), fixtures::slot(&pending, id)?)?;
        let expected_meta = fixtures::log_state(
            f::log_meta(&dir)?,
            wf_ports::log::RaftLogState::new_with_anchor(
                transition.before().vote(),
                Some(f::at(1)),
                Some(f::at(1)),
                Some((f::at(1), id)),
            ),
        );
        let (mut log, mut state) = f::open_pair(&dir)?;
        state.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
        assert_eq!(
            state.snapshot_transition(SnapshotLimits::default())?,
            Some(transition)
        );
        assert_ne!(fixtures::by_id(&mut state, old_id)?, []);
        assert_eq!(fixtures::by_id(&mut state, id)?, wire);
        if published {
            log.purge_through(f::at(1), id)?;
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
        assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
        assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        assert_eq!(fixtures::by_id(&mut state, id)?, wire);
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!(f::log_meta(&dir)?, expected_meta);
        assert!(f::log_rows(&dir)?.is_empty());
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn state_only_pins_with_no_log_purge_preserve_same_or_distinct_initial_slots() -> TestResult {
    for initial_pin in [false, true] {
        let dir = f::directory("advance-orphan-pin")?;
        let (mut log, mut state) = f::create_pair(&dir)?;
        let initial = if initial_pin {
            let wire = fixtures::artifact(&mut state, true)?;
            let id = fixtures::facts(&wire)?.0;
            state.pin_snapshot(id, SnapshotLimits::default())?;
            Some((id, wire))
        } else {
            None
        };
        let blank = f::blank(EntryStamp::Unstamped)?;
        f::append(&mut log, 0, blank.as_bytes())?;
        f::apply_control(&mut state, 0, blank.as_bytes())?;
        log.save_committed(f::at(0))?;
        state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
        let wire = fixtures::artifact(&mut state, true)?;
        let id = fixtures::facts(&wire)?.0;
        if !initial_pin {
            state.pin_snapshot(id, SnapshotLimits::default())?;
        }
        assert_eq!(log.state()?.last_purged(), None);
        let transition = SnapshotTransition::local(log.state()?, f::at(0), id)?;
        drop((log, state));
        let before = fixtures::cohort(&dir)?;
        let expected = fixtures::finalized(before.clone(), fixtures::slot(&before, id)?)?;
        let expected_meta = fixtures::log_state(
            f::log_meta(&dir)?,
            wf_ports::log::RaftLogState::new_with_anchor(
                transition.before().vote(),
                Some(f::at(0)),
                Some(f::at(0)),
                Some((f::at(0), id)),
            ),
        );
        let (log, state) = f::open_pair(&dir)?;
        let scope = f::scope()?;
        let (mut log, mut state) = purge_snapshot(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(0)));
        assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
        assert_eq!(fixtures::by_id(&mut state, id)?, wire);
        assert_eq!(transition.before().last_purged(), None);
        if let Some((old, _)) = initial {
            let mut bytes = Vec::new();
            assert!(
                state
                    .snapshot_by_id(old, &mut bytes, SnapshotLimits::default())?
                    .is_none()
            );
            assert_eq!(bytes, []);
        }
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!(f::log_meta(&dir)?, expected_meta);
        assert!(f::log_rows(&dir)?.is_empty());
        std::fs::remove_dir_all(dir)?;
    }
    applied_orphan_pending()?;
    Ok(())
}

fn applied_orphan_pending() -> TestResult {
    let dir = f::directory("advance-applied-orphan")?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 0, blank.as_bytes())?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    log.save_committed(f::at(0))?;
    let old_wire = fixtures::artifact(&mut state, true)?;
    let old_id = fixtures::facts(&old_wire)?.0;
    state.pin_snapshot(old_id, SnapshotLimits::default())?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    let member = f::member(EntryStamp::Stamped(f::clock(10, 0, 10)?), 4)?;
    f::append(&mut log, 1, member.as_bytes())?;
    f::apply_control(&mut state, 1, member.as_bytes())?;
    log.save_committed(f::at(1))?;
    let wire = fixtures::artifact(&mut state, true)?;
    let id = fixtures::facts(&wire)?.0;
    assert_ne!(id, old_id);
    let before_log = log.state()?;
    assert_eq!(before_log.last_purged(), None);
    let transition = SnapshotTransition::local(before_log, f::at(1), id)?;
    drop((log, state));
    let before = fixtures::cohort(&dir)?;
    let pending = fixtures::pending(before, transition)?;
    let expected = fixtures::finalized(pending.clone(), fixtures::slot(&pending, id)?)?;
    let old_meta = f::log_meta(&dir)?;
    let old_rows = f::log_rows(&dir)?;
    let expected_meta = fixtures::log_state(
        old_meta.clone(),
        wf_ports::log::RaftLogState::new_with_anchor(
            before_log.vote(),
            Some(f::at(1)),
            Some(f::at(1)),
            Some((f::at(1), id)),
        ),
    );
    let (mut log, mut state) = f::open_pair(&dir)?;
    state.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
    assert_eq!(
        state.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old_id));
    assert_eq!(fixtures::by_id(&mut state, old_id)?, old_wire);
    assert_eq!(fixtures::by_id(&mut state, id)?, wire);
    assert_eq!(log.state()?, before_log);
    drop((log, state));
    assert_eq!(fixtures::cohort(&dir)?, pending);
    assert_eq!(f::log_meta(&dir)?, old_meta);
    assert_eq!(f::log_rows(&dir)?, old_rows);
    let (log, state) = f::open_pair(&dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    assert_eq!(log.state()?.purged_snapshot_id(), Some(id));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    assert_eq!(fixtures::by_id(&mut state, id)?, wire);
    let mut retired = Vec::new();
    assert!(
        state
            .snapshot_by_id(old_id, &mut retired, SnapshotLimits::default())?
            .is_none()
    );
    assert_eq!(retired, []);
    drop((log, state));
    assert_eq!(fixtures::cohort(&dir)?, expected);
    assert_eq!(f::log_meta(&dir)?, expected_meta);
    assert!(f::log_rows(&dir)?.is_empty());
    drop(f::open_pair(&dir)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn retained_p_hlc_floor_survives_an_unstamped_candidate_and_later_valid_live_state() -> TestResult {
    assert_retained_floor(
        "hlc",
        Error::Recovery(CommittedRecoveryError::ClockMismatch),
    )
}

#[test]
fn retained_p_wall_floor_survives_an_unstamped_candidate_and_later_valid_live_state() -> TestResult
{
    assert_retained_floor(
        "wall",
        Error::Recovery(CommittedRecoveryError::ClockMismatch),
    )
}

#[test]
fn equal_p_requires_the_complete_original_control_despite_valid_later_live_state() -> TestResult {
    assert_retained_floor("control", Error::AnchorMismatch)
}

#[test]
fn equal_p_requires_the_original_artifact_even_when_its_complete_control_matches() -> TestResult {
    assert_retained_floor("artifact", Error::AnchorMismatch)
}

#[test]
fn retained_p_rejects_a_current_artifact_older_than_its_purged_base() -> TestResult {
    assert_retained_floor("older", Error::AnchorMismatch)
}

#[test]
fn retained_p_rejects_an_unapplied_current_artifact() -> TestResult {
    assert_retained_floor("unapplied", Error::AnchorMismatch)
}

fn assert_retained_floor(case: &str, expected: Error) -> TestResult {
    let dir = f::directory(case)?;
    fixtures::retained_floor(&dir, case)?;
    let before = fixtures::cohort(&dir)?;
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    let scope = f::scope()?;
    for purge in [false, true] {
        let (log, state) = f::open_pair(&dir)?;
        let result = if purge {
            purge_snapshot(
                log,
                state,
                scope.group(),
                scope.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )
        } else {
            recover_snapshotted(
                log,
                state,
                scope.group(),
                scope.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )
        };
        let error = result
            .err()
            .ok_or_else(|| format!("retained P {case} mismatch accepted; purge={purge}"))?;
        assert_eq!(error, expected);
        // Both owners must drop on refusal; strict reopen performs no repair.
        drop(f::open_pair(&dir)?);
        assert_eq!(fixtures::cohort(&dir)?, before);
        assert_eq!(f::log_meta(&dir)?, meta);
        assert_eq!(f::log_rows(&dir)?, rows);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
