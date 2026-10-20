//! Scalar catalog accounts, prospective ownership and actual target profiles.

use std::path::Path;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::PolicySchemaLimits;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::{Create, Operation};
use wf_ports::log::RaftLogState;
use wf_ports::snapshot::{SnapshotTransition, StateSnapshotTransitionStore};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_record_batch_transaction};

use super::super::limits::profile;
use super::{
    ALT_META, ALT_ROWS, RawCohort, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult,
    assert_advanced, catalog_bytes, f, manifest_version, s,
};

fn limited(error: SnapshotError) {
    assert!(
        matches!(
            error,
            SnapshotError::Limit
                | SnapshotError::State(StateTransactionError::Limit)
                | SnapshotError::Stage(StateTransactionError::Limit)
        ),
        "{error:?}"
    );
}

#[test]
fn scalar_upgrade_reserves_r_one_and_typed_export_requires_the_full_owner() -> TestResult {
    let dir = f::directory("advance-scalar-upgrade")?;
    let path = dir.join("state.redb");
    drop(s::empty(&path)?);
    let wire = s::capture(&path)?;
    let id: [u8; 32] = Sha256::digest(&wire).into();
    let before = s::raw(&path)?;
    let c = f::MIB;
    let w = 514 + 2048 + 1 + c;
    assert_eq!(w, 1_051_139);
    let data = (3, 325, 118);
    let short = profile(data, (1000, 1), (1, c), w - 1)?;
    limited(
        f::open(&path)?
            .upgrade_snapshot_layout(short)
            .err()
            .ok_or("short W accepted")?,
    );
    s::assert_raw(&path, &before)?;
    let exact = profile(data, (1000, 1), (1, c), w)?;
    let mut state = f::open(&path)?.upgrade_snapshot_layout(exact)?;
    assert!(state.supports_snapshot_transitions()?);
    assert_eq!(state.snapshot_transition(exact)?, None);
    let mut output = Vec::new();
    limited(
        state
            .snapshot_by_id(id, &mut output, exact)
            .err()
            .ok_or("R1 exported Metadata")?,
    );
    assert_eq!(output, [] as [u8; 0]);
    for (r, succeeds) in [(2049, false), (2050, true)] {
        let cap = profile(data, (325, 1), (r, c), 514 + 2048 + r + c)?;
        let result = state.snapshot_by_id(id, &mut output, cap);
        if succeeds {
            let metadata = result?.ok_or("original artifact absent")?;
            assert_eq!(metadata.id(), &id);
            assert!(metadata.control().last_entry().is_none());
            assert_eq!(output, wire);
        } else {
            limited(result.err().ok_or("short Metadata reservation accepted")?);
            assert_eq!(output, [] as [u8; 0]);
        }
    }
    drop(state);
    assert_advanced(&path, &super::expected_upgrade(before, super::INITIAL)?)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

pub(super) fn prepared_target(
    path: &Path,
) -> TestResult<(RedbStateStore, SnapshotTransition, Vec<u8>)> {
    let mut state = s::create(path)?;
    s::apply(&mut state, b"target", 0, 10)?;
    let mut wire = Vec::new();
    let metadata = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let id = *metadata.id();
    drop(metadata);
    let before = RaftLogState::new(None, Some(f::log(0)), Some(f::log(0)));
    let transition = SnapshotTransition::local(before, f::log(0), id)?;
    Ok((
        state.upgrade_snapshot_layout(SnapshotLimits::default())?,
        transition,
        wire,
    ))
}

pub(super) fn acknowledged(t: SnapshotTransition) -> RaftLogState {
    // Scalar component input; node tests separately prove the actual log commit.
    RaftLogState::new_with_anchor(
        t.before().vote(),
        t.before().committed(),
        t.before().last_log(),
        Some((t.position(), *t.id())),
    )
}

#[test]
fn scalar_prepare_finish_use_exact_workspace_and_preserve_pending_custody() -> TestResult {
    let dir = f::directory("advance-scalar-intent")?;
    let path = dir.join("state.redb");
    let (mut state, t, wire) = prepared_target(&path)?;
    let data = (1_000_000, 1_073_741_824, 67_108_864);
    let c = f::MIB;
    let w = 514 + 2048 + 1 + c;
    let exact = profile(data, (1000, 1), (1, c), w)?;
    let short = profile(data, (1000, 1), (1, c), w - 1)?;
    limited(
        state
            .prepare_snapshot_transition(t, short)
            .err()
            .ok_or("short prepare W accepted")?,
    );
    assert_eq!(state.snapshot_transition(exact)?, None);
    state.prepare_snapshot_transition(t, exact)?;
    state.prepare_snapshot_transition(t, exact)?;
    assert_eq!(state.snapshot_transition(exact)?, Some(t));
    state = pending_data_refusals(state, t, &path)?;
    let mut forbidden = Vec::new();
    assert!(matches!(
        state.capture_snapshot(&mut forbidden, SnapshotLimits::default()),
        Err(SnapshotError::TransitionPending)
    ));
    assert_eq!(forbidden, []);
    assert!(matches!(
        state.install_snapshot(
            s::stage(&dir.join("blocked.redb"), &wire)?,
            SnapshotLimits::default()
        ),
        Err(SnapshotError::TransitionPending)
    ));
    let wrong = RaftLogState::new_with_anchor(
        None,
        Some(f::log(0)),
        Some(f::log(0)),
        Some((f::log(0), [0; 32])),
    );
    assert!(matches!(
        state.finish_snapshot_transition(wrong, exact),
        Err(SnapshotError::TransitionConflict)
    ));
    assert_eq!(state.snapshot_transition(exact)?, Some(t));
    limited(
        state
            .finish_snapshot_transition(acknowledged(t), short)
            .err()
            .ok_or("short finish W accepted")?,
    );
    assert_eq!(state.snapshot_transition(exact)?, Some(t));
    state.finish_snapshot_transition(acknowledged(t), exact)?;
    assert_eq!(state.snapshot_transition(exact)?, None);
    assert_eq!(state.snapshot_pin(exact)?, Some(*t.id()));
    let mut retained = Vec::new();
    state
        .snapshot_by_id(*t.id(), &mut retained, SnapshotLimits::default())?
        .ok_or("same-slot target retired")?;
    assert_eq!(retained, wire);
    drop(state);
    let complete = s::raw(&path)?;
    assert!(complete[ALT_META].is_empty());
    assert!(complete[ALT_ROWS].is_empty());
    assert_advanced(&path, &complete)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn pending_data_refusals(
    mut state: RedbStateStore,
    t: SnapshotTransition,
    path: &Path,
) -> TestResult<RedbStateStore> {
    drop(state);
    let pending = s::raw(path)?;
    state = f::open(path)?;
    let c = f::MIB;
    let w = 514 + 2048 + 1 + c;
    for data in [
        (1, 1_073_741_824, 67_108_864),
        (1_000_000, 1, 67_108_864),
        (1_000_000, 1_073_741_824, 1),
    ] {
        let tight = profile(data, (1000, 1), (1, c), w)?;
        limited(
            state
                .prepare_snapshot_transition(t, tight)
                .err()
                .ok_or("exact pending retry bypassed requested target DATA")?,
        );
        assert!(state.supports_snapshot_transitions()?);
        assert_eq!(
            state.snapshot_transition(SnapshotLimits::default())?,
            Some(t)
        );
        drop(state);
        assert_advanced(path, &pending)?;
        state = f::open(path)?;
    }
    Ok(state)
}

#[test]
fn catalog_work_refusals_leave_every_table_and_the_ready_mode_intact() -> TestResult {
    let dir = f::directory("advance-catalog-work")?;
    let path = dir.join("state.redb");
    let (state, t, _) = prepared_target(&path)?;
    drop(state);
    let before = s::raw(&path)?;
    for work in [
        (1, 4_000_000_000, 100_000_000),
        (8_000_000, 1, 100_000_000),
        (8_000_000, 4_000_000_000, 1),
    ] {
        let cap = SnapshotLimits::new(
            (1_000_000, 1_073_741_824, 67_108_864),
            work,
            (1000, 65_536),
            (1, 64 * f::MIB),
            128 * f::MIB,
        )?;
        let mut state = f::open(&path)?;
        limited(
            state
                .prepare_snapshot_transition(t, cap)
                .err()
                .ok_or("tiny work accepted")?,
        );
        assert!(state.supports_snapshot_transitions()?);
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        drop(state);
        assert_advanced(&path, &before)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn apply_large(state: &mut RedbStateStore) -> TestResult {
    let frame = f::frame(
        b"large",
        1,
        vec![(
            b"large",
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: Some(vec![7; f::MIB]),
            }),
        )],
    )?;
    let result = apply_record_batch_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::log(0),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("large create was not retained".into());
    };
    assert_eq!(outcome.response(), [0]);
    Ok(())
}

fn authority_facts(authority: &RawCohort, wire: &[u8]) -> TestResult<(usize, usize, usize)> {
    let rows = authority
        .values()
        .map(std::collections::BTreeMap::len)
        .sum();
    let pair = authority
        .values()
        .flat_map(|rows| rows.iter())
        .map(|(k, v)| k.len() + v.len())
        .max()
        .ok_or("no authority")?;
    Ok((rows, wire.len(), pair))
}

#[test]
fn requested_small_data_does_not_reject_the_retained_large_old_anchor() -> TestResult {
    let dir = f::directory("advance-old-large-data")?;
    let target = dir.join("target.redb");
    let mut state = s::create(&target)?;
    apply_large(&mut state)?;
    let mut old_wire = Vec::new();
    let old = state.capture_snapshot(&mut old_wire, SnapshotLimits::default())?;
    let old_id = *old.id();
    drop(old);
    state.pin_snapshot(old_id, SnapshotLimits::default())?;
    drop(state.upgrade_snapshot_layout(SnapshotLimits::default())?);
    let before = s::raw(&target)?;
    let source = dir.join("source.redb");
    let mut producer = s::create(&source)?;
    s::apply(&mut producer, b"small-one", 0, 2)?;
    s::apply(&mut producer, b"small-two", 1, 3)?;
    drop(producer);
    let new_wire = s::capture(&source)?;
    let authority = s::authority(&s::raw(&source)?)?;
    assert_eq!(s::frame((b"g", b"z"), &authority)?, new_wire);
    let data = authority_facts(&authority, &new_wire)?;
    assert!(old_wire.len() > data.1);
    let c = 64 * f::MIB;
    let r = 4096;
    let cap = profile(data, (10_000_000, 65_536), (r, c), 2 * 514 + 2048 + r + c)?;
    let incoming = s::stage(&dir.join("incoming.redb"), &new_wire)?;
    let mut state = f::open(&target)?;
    let installed = state.install_snapshot(incoming, cap)?;
    assert_eq!(
        installed.id().as_slice(),
        Sha256::digest(&new_wire).as_slice()
    );
    assert_eq!(
        installed
            .control()
            .last_entry()
            .ok_or("missing new A")?
            .log(),
        f::log(1)
    );
    drop(installed);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old_id));
    let mut retained = Vec::new();
    state
        .snapshot_by_id(old_id, &mut retained, SnapshotLimits::default())?
        .ok_or("old large anchor disappeared")?;
    assert_eq!(retained, old_wire);
    drop(state);
    let mut expected = before.clone();
    let mut live = authority[s::META].clone();
    let manifest = manifest_version(&live[b"manifest".as_slice()], 4);
    live.insert(b"manifest".to_vec(), manifest);
    let old_rows = before[s::SNAP_META].len() + before[s::SNAP_ROWS].len();
    live.insert(
        b"snapshot".to_vec(),
        catalog_bytes(
            [Some((old_rows, &old_wire)), Some((data.0, &new_wire))],
            2,
            1,
            None,
        )?,
    );
    expected.insert(s::META.to_owned(), live);
    expected.insert(s::ROWS.to_owned(), authority[s::ROWS].clone());
    expected.insert(ALT_META.to_owned(), authority[s::META].clone());
    expected.insert(ALT_ROWS.to_owned(), authority[s::ROWS].clone());
    assert_advanced(&target, &expected)?;
    assert_eq!(expected[s::SNAP_META], before[s::SNAP_META]);
    assert_eq!(expected[s::SNAP_ROWS], before[s::SNAP_ROWS]);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn install_uses_the_actual_active_temporary_cap_instead_of_the_stages_history() -> TestResult {
    let dir = f::directory("advance-actual-target-c")?;
    let source = dir.join("source.redb");
    let mut producer = s::create(&source)?;
    apply_large(&mut producer)?;
    drop(producer);
    let wire = s::capture(&source)?;
    let target = dir.join("target.redb");
    drop(s::create(&target)?);
    let c = 256 * 1024;
    let schema = PolicySchemaLimits::new(PolicyLimits::default(), u64::try_from(c)?)?;
    let target_cap = StateStoreLimits::new(
        AdmissionLimits::default(),
        schema,
        (1000, 128 * f::MIB),
        (1_000_000, 1024 * f::MIB, 10_000_000),
        c,
        256 * f::MIB,
    )?;
    let scope = f::scope()?;
    let state = RedbStateStore::open(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        target_cap,
    )?;
    drop(state.upgrade_snapshot_layout(SnapshotLimits::default())?);
    let before = s::raw(&target)?;
    let incoming = s::stage(&dir.join("incoming.redb"), &wire)?;
    let mut state = RedbStateStore::open(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        target_cap,
    )?;
    assert!(matches!(
        state.install_snapshot(incoming, SnapshotLimits::default()),
        Err(SnapshotError::Stage(StateTransactionError::Limit))
    ));
    assert!(state.supports_snapshot_transitions()?);
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    drop(state);
    assert_advanced(&target, &before)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
