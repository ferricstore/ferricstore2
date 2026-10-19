//! A matching active checkpoint cannot certify the older frozen checkpoint.

use super::{
    CommittedRecoveryError, Error, RaftLogStore, SnapshotLimits, StateSnapshotStore, TestResult,
    caps, f, fixtures, purge_snapshot,
};
use wf_consensus::recovery::{CheckpointJoinError, CheckpointJoinLimits, verify_checkpoint};
use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_node::recovery::recover_committed;
use wf_ports::log::RaftLogEntry;
use wf_ports::state::GroupApplyStore;
use wf_types::version::{AppliedPosition, LogIdentity};

#[test]
fn snapshot_provenance_checks_membership_and_independent_clocks_at_s_despite_valid_a() -> TestResult
{
    for case in ["stale-membership", "false-none", "hlc", "wall"] {
        let dir = f::directory(case)?;
        let (mut log, mut state) = f::create_pair(&dir)?;
        let plain = f::blank(EntryStamp::Unstamped)?;
        let actual = match case {
            "hlc" => f::blank(EntryStamp::Stamped(f::clock(101, 0, 80)?))?,
            "wall" => f::blank(EntryStamp::Stamped(f::clock(100, 0, 90)?))?,
            _ => f::member(EntryStamp::Unstamped, 4)?,
        };
        let alternate = match case {
            "stale-membership" => f::member(EntryStamp::Unstamped, 5)?,
            "false-none" => f::blank(EntryStamp::Unstamped)?,
            _ => f::blank(EntryStamp::Stamped(f::clock(100, 0, 80)?))?,
        };
        f::append(&mut log, 0, actual.as_bytes())?;
        f::apply_control(&mut state, 0, alternate.as_bytes())?;
        f::append(&mut log, 1, plain.as_bytes())?;
        f::apply_control(&mut state, 1, plain.as_bytes())?;
        fixtures::capture(&mut state)?;
        let final_member = f::member(EntryStamp::Stamped(f::clock(200, 0, 200)?), 6)?;
        f::append(&mut log, 2, final_member.as_bytes())?;
        f::apply_control(&mut state, 2, final_member.as_bytes())?;
        log.save_committed(f::at(2))?;
        // Full A audit accepts: A has the exact body, last M and higher clocks.
        let s = f::scope()?;
        drop(recover_committed(
            log,
            state,
            s.group(),
            s.recovery(),
            caps()?,
        )?);
        let expected = if case == "hlc" || case == "wall" {
            CommittedRecoveryError::ClockMismatch
        } else {
            CommittedRecoveryError::MembershipMismatch
        };
        for purge in [false, true] {
            fixtures::assert_refusal(
                &dir,
                purge,
                caps()?,
                SnapshotLimits::default(),
                Error::Recovery(expected),
            )?;
        }
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn snapshot_local_provenance_binds_full_s_identity_actual_body_and_original_stamp() -> TestResult {
    for case in ["term", "node", "body", "stamp"] {
        let dir = f::directory(case)?;
        let (mut log, mut state) = f::create_pair(&dir)?;
        let plain = f::blank(EntryStamp::Unstamped)?;
        let other = f::blank(EntryStamp::Stamped(f::clock(0, 0, 0)?))?;
        let identity = match case {
            "term" => LogIdentity::new(2, 7, AppliedPosition::new(0)),
            "node" => LogIdentity::new(1, 8, AppliedPosition::new(0)),
            _ => f::at(0),
        };
        log.append(vec![RaftLogEntry::new(identity, plain.as_bytes().to_vec())])?;
        let control = f::apply_control(
            &mut state,
            0,
            if case == "body" {
                other.as_bytes()
            } else {
                plain.as_bytes()
            },
        )?;
        if case == "stamp" {
            drop((log, state));
            let changed = ControlState::restored(
                control.time(),
                control.last_entry().ok_or("no S")?,
                EntryStamp::Stamped(f::clock(0, 0, 0)?),
            )?;
            fixtures::replace_control(&dir, changed, false)?;
            (log, state) = f::open_pair(&dir)?;
        }
        fixtures::capture(&mut state)?;
        let end = LogIdentity::new(3, 7, AppliedPosition::new(1));
        log.append(vec![RaftLogEntry::new(end, plain.as_bytes().to_vec())])?;
        // Final A must match the actual higher leader identity, unlike S.
        let s = f::scope()?;
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            wf_domain::workflow::admission::AdmissionLimits::default(),
            10_000,
            64 * f::MIB,
            32 * f::MIB,
        )?;
        wf_consensus::apply::apply_control_entry(
            tx,
            s.group(),
            s.recovery(),
            plain.as_bytes(),
            end,
            wf_ports::consensus::RaftMembershipLimits::default(),
            256 * f::MIB,
        )?;
        log.save_committed(end)?;
        drop(verify_checkpoint(
            log,
            state,
            s.group(),
            s.recovery(),
            CheckpointJoinLimits::default(),
        )?);
        let expected = match case {
            "term" | "node" => CommittedRecoveryError::IdentityMismatch,
            "body" => CommittedRecoveryError::BodyMismatch,
            _ => CommittedRecoveryError::StampMismatch,
        };
        for purge in [false, true] {
            fixtures::assert_refusal(
                &dir,
                purge,
                caps()?,
                SnapshotLimits::default(),
                Error::Recovery(expected),
            )?;
        }
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn missing_unapplied_unpinned_and_mismatched_snapshot_cannot_authorize_purge_or_startup()
-> TestResult {
    for case in [
        "missing",
        "anchor-missing",
        "unapplied",
        "unpinned",
        "wrong-id",
        "wrong-boundary",
    ] {
        let dir = f::directory(case)?;
        let (mut log, mut state) = f::create_pair(&dir)?;
        if case == "unapplied" {
            fixtures::capture(&mut state)?;
        }
        let plain = f::blank(EntryStamp::Unstamped)?;
        for index in 0..=1 {
            f::append(&mut log, index, plain.as_bytes())?;
            f::apply_control(&mut state, index, plain.as_bytes())?;
        }
        log.save_committed(f::at(1))?;
        let mut id = [0; 32];
        if case != "missing" && case != "anchor-missing" && case != "unapplied" {
            id = fixtures::capture(&mut state)?;
        }
        if case == "wrong-id" || case == "wrong-boundary" {
            state.pin_snapshot(id, SnapshotLimits::default())?;
        }
        if case == "unpinned"
            || case == "wrong-id"
            || case == "wrong-boundary"
            || case == "anchor-missing"
        {
            let boundary = if case == "wrong-boundary" {
                f::at(0)
            } else {
                f::at(1)
            };
            log.purge_through(boundary, if case == "wrong-id" { [9; 32] } else { id })?;
        }
        drop((log, state));
        let expected = match case {
            "missing" | "anchor-missing" => Error::MissingSnapshot,
            "unapplied" => Error::UnappliedSnapshot,
            _ => Error::AnchorMismatch,
        };
        fixtures::assert_refusal(
            &dir,
            case == "missing" || case == "unapplied",
            caps()?,
            SnapshotLimits::default(),
            expected,
        )?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn old_no_purge_entry_points_refuse_even_when_a_is_retained_above_p() -> TestResult {
    let dir = f::directory("old-api-purged")?;
    let (log, state, _) = fixtures::seeded(&dir)?;
    let s = f::scope()?;
    drop(purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?);
    let before = fixtures::cohort(&dir)?;
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    assert_eq!(rows.keys().copied().collect::<Vec<_>>(), vec![1]);
    let (log, state) = f::open_pair(&dir)?;
    let error = verify_checkpoint(
        log,
        state,
        s.group(),
        s.recovery(),
        CheckpointJoinLimits::default(),
    )
    .err()
    .ok_or("old point join accepted purged history")?;
    assert_eq!(error, CheckpointJoinError::IncompatibleCheckpoint);
    let (log, state) = f::open_pair(&dir)?;
    let error = recover_committed(log, state, s.group(), s.recovery(), caps()?)
        .err()
        .ok_or("old replay accepted purged history")?;
    assert_eq!(error, CommittedRecoveryError::IncompatibleCheckpoint);
    drop(f::open_pair(&dir)?);
    assert_eq!(fixtures::cohort(&dir)?, before);
    assert_eq!(f::log_meta(&dir)?, meta);
    assert_eq!(f::log_rows(&dir)?, rows);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
