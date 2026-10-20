//! Received DATA containing Core ETF receipts reconciles both durable owners.

use super::{
    Cohort, SnapshotLimits, StateSnapshotStore, TestResult, artifact, caps, cohort,
    expected_complete, expected_original, f, normal, received, seed,
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, PolicySchemaCharge,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, PolicyVersion};
use wf_domain::workflow::control::EntryIdentity;
use wf_domain::workflow::request::{RetainedOutcome, RetainedStatus};
use wf_node::recovery::{
    CommittedRecoveryError, SnapshotRecoveryError as Error, install_received_snapshot,
    recover_snapshotted,
};
use wf_ports::log::{
    RaftLogError, RaftLogState, RaftLogStore, prepare_complete_core_policy_patch_frame,
};
use wf_ports::snapshot::{SnapshotError, SnapshotTransition, StateSnapshotTransitionStore};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StateTransactionError, StoredPolicyValue,
};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    CorePolicyTransactionError, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction,
};
use wf_types::identity::RequestId;

mod pending;

fn raw(generation: u8) -> TestResult<Vec<u8>> {
    let mut wire = expected_original(generation)?;
    assert_eq!(wire.len(), 51);
    assert_eq!(&wire[21..26], &[116, 0, 0, 0, 2]);
    wire[25] = 3;
    wire.extend_from_slice(b"\x77\x07version\x61\x07");
    assert_eq!(wire.len(), 62);
    Ok(wire)
}

fn core() -> TestResult<Vec<u8>> {
    let limits = normal();
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            f::scope()?,
            RequestId::new(b"core-u".to_vec())?,
            f::clock(14, 0, 14)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"u".to_vec(),
                expected_generation: Some(2),
                patch: PolicyPatch {
                    version: FieldPatch::Set(PolicyVersion::Number(7)),
                    ..PolicyPatch::default()
                },
            },
        ),
        limits.schema,
        limits.codec.admission(),
        &mut PolicySchemaCharge::new(limits.schema),
    )?
    .as_bytes()
    .to_vec())
}

fn receipt(value: &RetainedOutcome) -> TestResult {
    assert_eq!(value.scope(), &f::scope()?);
    assert_eq!(value.request().as_bytes(), b"core-u");
    assert_eq!((value.intent().version(), value.response_version()), (2, 2));
    assert_eq!(value.status(), RetainedStatus::Accepted);
    assert_eq!(value.response(), raw(3)?);
    assert_eq!(
        (
            value.original_position(),
            value.original_time().get(),
            value.expires_at().get()
        ),
        (f::at(5), 14, 1014)
    );
    Ok(())
}

fn inspect(state: &mut RedbStateStore, position: u64) -> TestResult {
    let scope = f::scope()?;
    let limits = normal();
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let original = tx
        .stored_policy_value(scope.tenant(), b"u", limits.schema, &mut charge)?
        .ok_or("received original authority missing")?;
    let StoredPolicyValue::Original(original) = original else {
        return Err("received ETF became V2".into());
    };
    assert_eq!(original, raw(3)?);
    let complete = tx
        .stored_policy_value(scope.tenant(), b"r", limits.schema, &mut charge)?
        .ok_or("received complete authority missing")?;
    let StoredPolicyValue::Complete(complete) = complete else {
        return Err("unrelated V2 changed format".into());
    };
    assert_eq!(complete.encoded(), expected_complete(8)?);
    receipt(
        &tx.outcome(scope.tenant(), &RequestId::new(b"core-u".to_vec())?)?
            .ok_or("received response2 missing")?,
    )?;
    for (request, index, time, generation) in
        [(b"set".as_slice(), 2, 12, 1), (b"replace", 3, 13, 2)]
    {
        let value = tx
            .outcome(scope.tenant(), &RequestId::new(request.to_vec())?)?
            .ok_or("received original response1 missing")?;
        assert_eq!((value.intent().version(), value.response_version()), (4, 1));
        assert_eq!(value.response(), expected_original(generation)?);
        assert_eq!(
            (
                value.original_position(),
                value.original_time().get(),
                value.expires_at().get()
            ),
            (f::at(index), time, time + 1000)
        );
    }
    drop(tx);
    assert_eq!(
        f::applied(state)?.0.last_entry().map(EntryIdentity::log),
        Some(f::at(position))
    );
    Ok(())
}

fn source(dir: &Path) -> TestResult<Vec<u8>> {
    let (old, intermediate, position) = seed(dir, true, true)?;
    assert_eq!(position, 2);
    assert_eq!(&old[..6], b"FWSP\0\x02");
    let (mut log, mut state) = f::open_pair(dir)?;
    log.truncate_from(5)?;
    let body = core()?;
    f::append(&mut log, 5, &body)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } =
        apply_complete_core_policy_patch_transaction(
            &mut state,
            &f::scope()?,
            &body,
            f::at(5),
            normal(),
        )?
    else {
        return Err("source response2 not retained".into());
    };
    receipt(&outcome)?;
    log.save_committed(f::at(5))?;
    let mut wire = Vec::new();
    let id = *state
        .capture_snapshot(&mut wire, SnapshotLimits::default())?
        .id();
    assert_eq!(&wire[..6], b"FWSP\0\x03");
    assert_eq!(id, <[u8; 32]>::from(Sha256::digest(&wire)));
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(&old).into())
    );
    artifact(&mut state, &old)?;
    artifact(&mut state, &wire)?;
    assert!(
        state
            .snapshot_by_id(
                Sha256::digest(&intermediate).into(),
                &mut Vec::new(),
                SnapshotLimits::default()
            )?
            .is_none()
    );
    inspect(&mut state, 5)?;
    Ok(wire)
}

fn expected(
    before: &Cohort,
    incoming: &Cohort,
    old: &[u8],
    wire: &[u8],
    transition: SnapshotTransition,
) -> TestResult<(Cohort, Cohort)> {
    let old_facts = (
        Sha256::digest(old).into(),
        before[2].as_ref().ok_or("old metadata")?.len()
            + before[3].as_ref().ok_or("old rows")?.len(),
        old.len(),
    );
    let new_facts = (
        Sha256::digest(wire).into(),
        incoming[4].as_ref().ok_or("incoming metadata")?.len()
            + incoming[5].as_ref().ok_or("incoming rows")?.len(),
        wire.len(),
    );
    assert_eq!(incoming[1], incoming[5]);
    let mut pending = before.clone();
    for slot in [0, 1, 4, 5] {
        pending[slot].clone_from(&incoming[slot]);
    }
    pending[0].as_mut().ok_or("live metadata")?.insert(
        b"snapshot".to_vec(),
        received::descriptor([Some(old_facts), Some(new_facts)], 1, 0, Some(transition))?,
    );
    let mut finished = pending.clone();
    finished[2] = Some(f::Rows::new());
    finished[3] = Some(f::Rows::new());
    finished[0].as_mut().ok_or("live metadata")?.insert(
        b"snapshot".to_vec(),
        received::descriptor([None, Some(new_facts)], 1, 1, None)?,
    );
    Ok((pending, finished))
}

fn clean(dir: &Path, wire: &[u8]) -> TestResult {
    let candidate = received::stage(dir, wire)?;
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = install_received_snapshot(
        log,
        state,
        candidate,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(5)));
    inspect(&mut state, 5)?;
    Ok(())
}

fn pending(dir: &Path, old: &[u8], wire: &[u8], transition: SnapshotTransition) -> TestResult {
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
    assert_eq!(
        apply_complete_core_policy_patch_transaction(
            &mut state,
            &f::scope()?,
            &core()?,
            f::at(6),
            normal()
        ),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::InvalidChanges
        ))
    );
    drop((log, state));
    Ok(())
}

fn finish(dir: &Path, old: &[u8], wire: &[u8], expected: &Cohort) -> TestResult {
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
        assert_eq!(log.state()?.last_purged(), Some(f::at(5)));
        assert_eq!(log.state()?.committed(), Some(f::at(5)));
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        assert_eq!(
            state.snapshot_pin(SnapshotLimits::default())?,
            Some(Sha256::digest(wire).into())
        );
        artifact(&mut state, wire)?;
        assert!(
            state
                .snapshot_by_id(
                    Sha256::digest(old).into(),
                    &mut Vec::new(),
                    SnapshotLimits::default()
                )?
                .is_none()
        );
        inspect(&mut state, 5)?;
        drop((log, state));
        assert_eq!(cohort(dir)?, *expected);
        assert!(f::log_rows(dir)?.is_empty());
    }
    let logs = (f::log_rows(dir)?, f::log_meta(dir)?);
    clean(dir, wire)?;
    assert_eq!(cohort(dir)?, *expected);
    assert_eq!((f::log_rows(dir)?, f::log_meta(dir)?), logs);
    Ok(())
}

fn failed(dir: &Path, wire: &[u8], mode: u8, before: RaftLogState) -> TestResult {
    let candidate = received::stage(dir, wire)?;
    let (log, mut state) = f::open_pair(dir)?;
    drop(log);
    let fault = Arc::new(AtomicU8::new(0));
    let mut log = received::fault_log(dir, Arc::clone(&fault))?;
    let scope = f::scope()?;
    if mode == 2 {
        let id = Sha256::digest(wire).into();
        let transition = SnapshotTransition::received(before, f::at(5), id)?;
        state.prepare_received_snapshot(candidate, transition, SnapshotLimits::default())?;
        let control = f::applied(&mut state)?.0;
        let hash = *control
            .last_entry()
            .ok_or("received control")?
            .prepared()
            .as_bytes();
        fault.store(4, Ordering::SeqCst);
        assert_eq!(
            log.install_snapshot_boundary(before, f::at(5), hash, id),
            Err(RaftLogError::Uncertain)
        );
        assert_eq!(log.state(), Err(RaftLogError::Fenced));
        drop((log, state));
    } else {
        fault.store(if mode == 0 { 3 } else { 4 }, Ordering::SeqCst);
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
            .err(),
            Some(Error::Recovery(CommittedRecoveryError::Log(if mode == 0 {
                RaftLogError::Unavailable
            } else {
                RaftLogError::Uncertain
            })))
        );
    }
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "actual delegated fault consumed"
    );
    Ok(())
}

fn replay_after_install(dir: &Path, wire: &[u8]) -> TestResult {
    let scope = f::scope()?;
    let (mut log, state) = f::open_pair(dir)?;
    f::append(&mut log, 6, &core()?)?;
    log.save_committed(f::at(6))?;
    f::append(&mut log, 7, b"uncommitted received interop suffix")?;
    drop((log, state));
    let logs = (f::log_rows(dir)?, f::log_meta(dir)?);
    let mut expected = None;
    for _ in 0..2 {
        let (log, state) = f::open_pair(dir)?;
        let (log, mut state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        inspect(&mut state, 6)?;
        artifact(&mut state, wire)?;
        drop((log, state));
        let actual = cohort(dir)?;
        if let Some(ref expected) = expected {
            assert_eq!(&actual, expected);
        }
        expected = Some(actual);
        assert_eq!((f::log_rows(dir)?, f::log_meta(dir)?), logs);
    }
    Ok(())
}

#[test]
fn received_interop_cutover_reconciles_log_and_store_before_serving() -> TestResult {
    pending::assert_requests()?;
    let source_dir = f::directory("interop-received-source")?;
    let wire = source(&source_dir)?;
    let incoming = cohort(&source_dir)?;
    let reference = f::directory("interop-received-reference")?;
    let (log, state, old, _) = received::target(&reference)?;
    drop((log, state));
    clean(&reference, &wire)?;
    let ready = cohort(&reference)?;
    finish(&reference, &old, &wire, &ready)?;
    replay_after_install(&reference, &wire)?;
    let replayed = cohort(&reference)?;
    for mode in 0..3 {
        let dir = f::directory("interop-received-fault")?;
        let (mut log, state, target_old, _) = received::target(&dir)?;
        assert_eq!(target_old, old);
        f::append(&mut log, 1, b"obsolete uncommitted receiver tail")?;
        let before_log = log.state()?;
        drop((log, state));
        let before = cohort(&dir)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let transition =
            SnapshotTransition::received(before_log, f::at(5), Sha256::digest(&wire).into())?;
        let (pending_cohort, finished) = expected(&before, &incoming, &old, &wire, transition)?;
        assert_eq!(finished, ready);
        failed(&dir, &wire, mode, before_log)?;
        if mode == 0 {
            assert_eq!(cohort(&dir)?, before);
            assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
            clean(&dir, &wire)?;
        } else {
            assert_eq!(cohort(&dir)?, pending_cohort);
            pending(&dir, &old, &wire, transition)?;
            assert_eq!(cohort(&dir)?, pending_cohort);
        }
        finish(&dir, &old, &wire, &finished)?;
        replay_after_install(&dir, &wire)?;
        assert_eq!(cohort(&dir)?, replayed);
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(source_dir)?;
    Ok(())
}
