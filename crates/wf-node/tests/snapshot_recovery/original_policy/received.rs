//! Received original DATA stays recoverable across independent log failures.

use super::super::{
    CommittedRecoveryError, Error, SnapshotLimits, StateSnapshotStore, disk_backend,
    recover_snapshotted,
};
use super::{
    Cohort, TestResult, artifact, caps, cohort, f, inspect, legacy_base, normal, policy, seed,
};
use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_node::recovery::{CommittedRecoveryLimits, install_received_snapshot};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogState, RaftLogStore};
use wf_ports::snapshot::{SnapshotError, SnapshotTransition, StateSnapshotTransitionStore};
use wf_ports::state::StateTransactionError;
use wf_storage::log::RedbLogStore;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{
    OriginalPolicyTransactionError, apply_original_policy_set_transaction,
};

fn stage(dir: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let scope = f::scope()?;
    let path = dir.join("candidate.redb");
    // A failed consuming composition has dropped its private candidate owner.
    if path.try_exists()? {
        std::fs::remove_file(&path)?;
    }
    Ok(RedbSnapshotStage::receive(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}

fn target(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, Vec<u8>, u64)> {
    let (log, state, old, position) = legacy_base(dir, false)?;
    let mut state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    // Upgrade the live receiver capability while retaining its exact old DATA.
    artifact(&mut state, &old)?;
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(&old).into())
    );
    Ok((log, state, old, position))
}

fn fault_log(dir: &Path, fault: Arc<AtomicU8>) -> TestResult<RedbLogStore> {
    let scope = f::scope()?;
    Ok(RedbLogStore::open_database(
        disk_backend::fault_database(&dir.join("log.redb"), fault, 0)?,
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}

fn identity(out: &mut Vec<u8>, position: wf_types::version::LogIdentity) {
    for n in [
        position.term(),
        position.leader_node(),
        position.index().get(),
    ] {
        out.extend_from_slice(&n.to_be_bytes());
    }
}

fn descriptor(
    slots: [Option<([u8; 32], usize, usize)>; 2],
    current: u8,
    anchor: u8,
    pending: Option<SnapshotTransition>,
) -> TestResult<Vec<u8>> {
    let mut out = b"FWSD\0\x03".to_vec();
    out.extend_from_slice(&[current + 1, anchor + 1]);
    for slot in slots {
        out.push(u8::from(slot.is_some()));
        if let Some((id, rows, bytes)) = slot {
            out.extend_from_slice(&u64::try_from(rows)?.to_be_bytes());
            out.extend_from_slice(&u64::try_from(bytes)?.to_be_bytes());
            out.extend_from_slice(&id);
        }
    }
    out.push(u8::from(pending.is_some()));
    if let Some(t) = pending {
        let before = t.before();
        out.extend_from_slice(&[
            u8::from(t.is_received()),
            u8::from(before.vote().is_some())
                | (u8::from(before.committed().is_some()) << 1)
                | (u8::from(before.last_log().is_some()) << 2)
                | (u8::from(before.last_purged().is_some()) << 3),
        ]);
        if let Some(vote) = before.vote() {
            out.extend_from_slice(&vote.term().to_be_bytes());
            out.extend_from_slice(&vote.node().to_be_bytes());
            out.push(u8::from(vote.committed()));
        }
        for position in [before.committed(), before.last_log()]
            .into_iter()
            .flatten()
        {
            identity(&mut out, position);
        }
        if let Some((position, id)) = before.last_purged().zip(before.purged_snapshot_id()) {
            identity(&mut out, position);
            out.extend_from_slice(&id);
        }
        identity(&mut out, t.position());
        out.extend_from_slice(t.id());
    }
    let scope = f::scope()?;
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-catalog-v1");
    for field in [scope.group().as_bytes(), scope.recovery().as_bytes()] {
        hash.update(u16::try_from(field.len())?.to_be_bytes());
        hash.update(field);
    }
    hash.update(&out);
    out.extend_from_slice(&hash.finalize());
    Ok(out)
}

fn pending_cohorts(
    before: &[Option<f::Rows>; 6],
    source: &[Option<f::Rows>; 6],
    old: &[u8],
    wire: &[u8],
    transition: SnapshotTransition,
) -> TestResult<(Cohort, Cohort)> {
    let old_facts = (
        Sha256::digest(old).into(),
        before[2].as_ref().ok_or("old meta")?.len() + before[3].as_ref().ok_or("old rows")?.len(),
        old.len(),
    );
    let new_facts = (
        Sha256::digest(wire).into(),
        source[4].as_ref().ok_or("new meta")?.len() + source[5].as_ref().ok_or("new rows")?.len(),
        wire.len(),
    );
    let mut pending = before.clone();
    pending[0].clone_from(&source[0]);
    pending[1].clone_from(&source[1]);
    pending[4].clone_from(&source[4]);
    pending[5].clone_from(&source[5]);
    pending[0].as_mut().ok_or("live meta")?.insert(
        b"snapshot".to_vec(),
        descriptor([Some(old_facts), Some(new_facts)], 1, 0, Some(transition))?,
    );
    let mut finished = pending.clone();
    finished[2] = Some(f::Rows::new());
    finished[3] = Some(f::Rows::new());
    finished[0].as_mut().ok_or("live meta")?.insert(
        b"snapshot".to_vec(),
        descriptor([None, Some(new_facts)], 1, 1, None)?,
    );
    Ok((pending, finished))
}

fn install_clean(dir: &Path, wire: &[u8]) -> TestResult {
    let candidate = stage(dir, wire)?;
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    drop(install_received_snapshot(
        log,
        state,
        candidate,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?);
    Ok(())
}

fn check_pending(
    dir: &Path,
    old: &[u8],
    wire: &[u8],
    transition: SnapshotTransition,
) -> TestResult {
    let (log, mut state) = f::open_pair(dir)?;
    assert_eq!(
        state.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    artifact(&mut state, old)?;
    artifact(&mut state, wire)?;
    assert_eq!(
        state
            .capture_snapshot(&mut Vec::new(), SnapshotLimits::default())
            .err(),
        Some(SnapshotError::TransitionPending)
    );
    let scope = f::scope()?;
    let body = policy(&scope, b"forbidden-pending", 13, true)?;
    let profile = normal();
    assert_eq!(
        apply_original_policy_set_transaction(
            &mut state,
            &scope,
            &body,
            f::at(2),
            profile,
            &mut PolicySchemaCharge::new(profile.schema)
        ),
        Err(OriginalPolicyTransactionError::Transaction(
            StateTransactionError::InvalidChanges
        ))
    );
    drop((log, state));
    Ok(())
}

fn finish(dir: &Path, wire: &[u8], expected: &[Option<f::Rows>; 6]) -> TestResult {
    let scope = f::scope()?;
    for _ in 0..2 {
        let (log, state) = f::open_pair(dir)?;
        let (mut log, mut state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        inspect(&mut state, 1, 1, false)?;
        artifact(&mut state, wire)?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
        assert_eq!(
            state.snapshot_pin(SnapshotLimits::default())?,
            Some(Sha256::digest(wire).into())
        );
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        drop((log, state));
        assert_eq!(cohort(dir)?, *expected);
    }
    let before = (f::log_rows(dir)?, f::log_meta(dir)?);
    install_clean(dir, wire)?;
    assert_eq!(cohort(dir)?, *expected);
    assert_eq!((f::log_rows(dir)?, f::log_meta(dir)?), before);
    Ok(())
}

fn failed_install(dir: &Path, wire: &[u8], mode: u8, before: RaftLogState) -> TestResult {
    let candidate = stage(dir, wire)?;
    let (old_log, mut state) = f::open_pair(dir)?;
    drop(old_log);
    let fault = Arc::new(AtomicU8::new(0));
    let mut log = fault_log(dir, Arc::clone(&fault))?;
    let scope = f::scope()?;
    if mode == 2 {
        let id = Sha256::digest(wire).into();
        let transition = SnapshotTransition::received(before, f::at(1), id)?;
        state.prepare_received_snapshot(candidate, transition, SnapshotLimits::default())?;
        let control = f::applied(&mut state)?.0;
        let hash = *control
            .last_entry()
            .ok_or("received control")?
            .prepared()
            .as_bytes();
        fault.store(4, Ordering::SeqCst);
        assert_eq!(
            log.install_snapshot_boundary(before, f::at(1), hash, id),
            Err(RaftLogError::Uncertain)
        );
        assert_eq!(log.state(), Err(RaftLogError::Fenced));
        drop((log, state));
    } else {
        fault.store(if mode == 0 { 3 } else { 4 }, Ordering::SeqCst);
        let error = install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )
        .err()
        .ok_or("actual log fault not refused")?;
        assert_eq!(
            error,
            Error::Recovery(CommittedRecoveryError::Log(if mode == 0 {
                RaftLogError::Unavailable
            } else {
                RaftLogError::Uncertain
            }))
        );
    }
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "actual delegated fault consumed"
    );
    Ok(())
}

fn one_pool(bytes: usize, owned: usize) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal(),
        (1000, bytes),
        owned,
        1024 * f::MIB,
    )?
    .with_core_policy_patches()
    .with_original_policy_sets())
}

fn suffix(dir: &Path, wire: &[u8]) -> TestResult {
    let scope = f::scope()?;
    let replace = policy(&scope, b"replace", 13, true)?;
    let blank = f::blank(wf_domain::workflow::control::EntryStamp::Unstamped)?;
    let (mut log, state) = f::open_pair(dir)?;
    f::append(&mut log, 2, &replace)?;
    f::append(&mut log, 3, blank.as_bytes())?;
    log.save_committed(f::at(3))?;
    drop((log, state));
    let before = cohort(dir)?;
    let logs = (f::log_rows(dir)?, f::log_meta(dir)?);
    let owned = 512 + replace.len().max(blank.as_bytes().len());
    let (log, state) = f::open_pair(dir)?;
    assert_eq!(
        recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            one_pool(replace.len() - 1, owned)?,
            SnapshotLimits::default()
        )
        .err(),
        Some(Error::Recovery(CommittedRecoveryError::Log(
            RaftLogError::Limit
        )))
    );
    drop(f::open_pair(dir)?);
    assert_eq!(cohort(dir)?, before);
    assert_eq!((f::log_rows(dir)?, f::log_meta(dir)?), logs);
    let mut after = None;
    for _ in 0..2 {
        let (log, state) = f::open_pair(dir)?;
        let (log, mut state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            one_pool(replace.len() + blank.as_bytes().len(), owned)?,
            SnapshotLimits::default(),
        )?;
        inspect(&mut state, 2, 1, false)?;
        artifact(&mut state, wire)?;
        assert_eq!(
            f::applied(&mut state)?
                .0
                .last_entry()
                .map(wf_domain::workflow::control::EntryIdentity::log),
            Some(f::at(3))
        );
        drop((log, state));
        let actual = cohort(dir)?;
        if let Some(expected) = after {
            assert_eq!(actual, expected);
        }
        after = Some(actual);
        assert_eq!((f::log_rows(dir)?, f::log_meta(dir)?), logs);
    }
    Ok(())
}

fn malformed_prefixes(dir: &Path, wire: &[u8]) -> TestResult {
    let (log, state, _, _) = target(dir)?;
    drop((log, state));
    let before = cohort(dir)?;
    let logs = (f::log_rows(dir)?, f::log_meta(dir)?);
    let mut trailing = wire.to_vec();
    trailing.push(0);
    for bad in [&wire[..wire.len() - 1], trailing.as_slice()] {
        let error = stage(dir, bad)
            .err()
            .ok_or("malformed complete snapshot accepted")?;
        assert_eq!(
            *error
                .downcast::<SnapshotError>()
                .map_err(|_| "snapshot error origin changed")?,
            SnapshotError::Malformed
        );
        assert_eq!(cohort(dir)?, before);
        assert_eq!((f::log_rows(dir)?, f::log_meta(dir)?), logs);
    }
    install_clean(dir, wire)?;
    Ok(())
}

#[test]
fn pending_received_original_lineage_and_failure_custody() -> TestResult {
    let source_dir = f::directory("original-received-source")?;
    let (old, wire, position) = seed(&source_dir, false, false)?;
    assert_eq!(position, 1);
    let source = cohort(&source_dir)?;
    let reference = f::directory("original-received-reference")?;
    let (log, state, reference_old, _) = target(&reference)?;
    assert_eq!(reference_old, old);
    drop((log, state));
    install_clean(&reference, &wire)?;
    let ready = cohort(&reference)?;
    finish(&reference, &wire, &ready)?;
    suffix(&reference, &wire)?;
    let expected_suffix = cohort(&reference)?;
    for mode in 0..3 {
        let dir = f::directory("original-received-fault")?;
        let (mut log, state, target_old, _) = target(&dir)?;
        assert_eq!(target_old, old);
        let before_log = log.state()?;
        drop((log, state));
        let before = cohort(&dir)?;
        let log_before = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let transition =
            SnapshotTransition::received(before_log, f::at(1), Sha256::digest(&wire).into())?;
        let (pending, finished) = pending_cohorts(&before, &source, &old, &wire, transition)?;
        assert_eq!(finished, ready);
        failed_install(&dir, &wire, mode, before_log)?;
        if mode == 0 {
            assert_eq!(cohort(&dir)?, before);
            assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), log_before);
            install_clean(&dir, &wire)?;
        } else {
            assert_eq!(cohort(&dir)?, pending);
            check_pending(&dir, &old, &wire, transition)?;
            assert_eq!(cohort(&dir)?, pending);
        }
        finish(&dir, &wire, &finished)?;
        suffix(&dir, &wire)?;
        assert_eq!(cohort(&dir)?, expected_suffix);
        std::fs::remove_dir_all(dir)?;
    }
    let bad = f::directory("original-received-malformed")?;
    malformed_prefixes(&bad, &wire)?;
    std::fs::remove_dir_all(bad)?;
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(source_dir)?;
    Ok(())
}
