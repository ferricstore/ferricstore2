//! Before-work workspace, one point/body pool and immutable legacy profiles.

use super::{
    CommittedRecoveryError, CommittedRecoveryLimits, Error, RaftLogStore, SnapshotLimits,
    StateSnapshotStore, TestResult, f, fixtures, recover_snapshotted,
};
use std::{
    io::Cursor,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use wf_domain::workflow::{
    Operation,
    control::{ControlState, EntryIdentity, EntryStamp},
};
use wf_node::recovery::{install_received_snapshot, restore_received_snapshot};
use wf_ports::{
    consensus::RaftMembershipLimits, log::RaftLogError, snapshot::StateSnapshotTransitionStore,
};
use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};

fn snapshot() -> TestResult<SnapshotLimits> {
    Ok(SnapshotLimits::new(
        (1_000_000, 1_073_741_824, 67_108_864),
        (8_000_000, 4_000_000_000, 100_000_000),
        (10_000_000, 65_536),
        (2050, f::MIB),
        2 * f::MIB,
    )?)
}

fn profile(
    work: (usize, usize),
    entry: usize,
    workspace: usize,
) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (100, 1536, 1024, 131),
        f::driver_limits(),
        work,
        entry,
        workspace,
    )?)
}

fn fault_stage(dir: &Path, wire: &[u8], fault: Arc<AtomicU8>) -> TestResult<RedbSnapshotStage> {
    let path = dir.join("fault-stage.redb");
    drop(redb::Builder::new().create(&path)?);
    let database = super::disk_backend::fault_database(&path, fault, 0)?;
    let scope = f::scope()?;
    Ok(RedbSnapshotStage::receive_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}

fn received_refusal(dir: &Path, wire: &[u8], fresh: bool, maximum: usize) -> TestResult {
    let before = if fresh {
        None
    } else {
        Some(fixtures::cohort(dir)?)
    };
    let legacy = if fresh {
        Some(super::super::fixtures::cohort(dir)?)
    } else {
        None
    };
    let meta = f::log_meta(dir)?;
    let rows = f::log_rows(dir)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut probe = fault_stage(dir, wire, Arc::clone(&fault))?;
    fault.store(3, Ordering::SeqCst);
    assert_eq!(
        probe.metadata(snapshot()?).err(),
        Some(wf_ports::snapshot::SnapshotError::Stage(
            wf_ports::state::StateTransactionError::Unavailable
        ))
    );
    drop(probe);
    std::fs::remove_file(dir.join("fault-stage.redb"))?;
    fault.store(0, Ordering::SeqCst);
    let candidate = fault_stage(dir, wire, Arc::clone(&fault))?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    fault.store(3, Ordering::SeqCst);
    let cap = profile((3, 39), 525, maximum)?;
    let result = if fresh {
        drop(state);
        restore_received_snapshot(
            log,
            candidate,
            scope.group(),
            scope.recovery(),
            cap,
            snapshot()?,
        )
    } else {
        install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            cap,
            snapshot()?,
        )
    };
    assert_eq!(
        result.err().ok_or("unpaid Stage owner accepted")?,
        Error::Recovery(CommittedRecoveryError::Limit)
    );
    // Metadata work would hit the calibrated Stage error before this Limit.
    // Consumed Stage cleanup can sync/read and reset the trap after refusal.
    if let Some(data) = before {
        assert_eq!(fixtures::cohort(dir)?, data);
    }
    if let Some(data) = legacy {
        assert_eq!(super::super::fixtures::cohort(dir)?, data);
    }
    assert_eq!(f::log_meta(dir)?, meta);
    assert_eq!(f::log_rows(dir)?, rows);
    let path = dir.join("fault-stage.redb");
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn received_success(
    dir: &Path,
    wire: &[u8],
    expected: &[f::Rows; 6],
    fresh: bool,
    exact: usize,
) -> TestResult {
    let meta = f::log_meta(dir)?;
    let candidate = super::received::stage(dir, wire)?;
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let cap = profile((3, 39), 525, exact)?;
    let (log, mut state) = if fresh {
        drop(state);
        restore_received_snapshot(
            log,
            candidate,
            scope.group(),
            scope.recovery(),
            cap,
            snapshot()?,
        )?
    } else {
        install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            cap,
            snapshot()?,
        )?
    };
    let id = fixtures::facts(wire)?.0;
    assert_eq!(fixtures::by_id(&mut state, id)?, wire);
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    drop((log, state));
    if fresh {
        std::fs::rename(dir.join("state.redb"), dir.join("unused-state.redb"))?;
        std::fs::rename(dir.join("candidate.redb"), dir.join("state.redb"))?;
    }
    assert_eq!(fixtures::cohort(dir)?, *expected);
    assert_eq!(
        f::log_meta(dir)?,
        fixtures::log_state(
            meta,
            wf_ports::log::RaftLogState::new_with_anchor(
                None,
                Some(f::at(0)),
                Some(f::at(0)),
                Some((f::at(0), id))
            )
        )
    );
    assert!(f::log_rows(dir)?.is_empty());
    Ok(())
}

#[test]
fn live_and_fresh_receive_pay_each_actual_namespace_before_source_reads() -> TestResult {
    let dir = f::directory("advance-stage-source")?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 0, blank.as_bytes())?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    log.save_committed(f::at(0))?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    let wire = fixtures::artifact(&mut state, true)?;
    drop((log, state));
    let expected = fixtures::finalized(fixtures::cohort(&dir)?, 0)?;
    for fresh in [false, true] {
        let target = f::directory("advance-stage-workspace")?;
        let (log, state) = f::create_pair(&target)?;
        let state = if fresh {
            state
        } else {
            state.upgrade_snapshot_layout(SnapshotLimits::default())?
        };
        drop((log, state));
        let exact = 4100 + 2050 + f::MIB + if fresh { 0 } else { 514 };
        assert_eq!(exact, if fresh { 1_054_726 } else { 1_055_240 });
        for maximum in [exact - 1, exact, with_headers(exact)? - 1] {
            received_refusal(&target, &wire, fresh, maximum)?;
        }
        received_success(&target, &wire, &expected, fresh, with_headers(exact)?)?;
        std::fs::remove_dir_all(target)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn seed(dir: &std::path::Path, upgraded: bool) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    for index in 0..=2 {
        f::append(&mut log, index, blank.as_bytes())?;
        if index < 2 {
            f::apply_control(&mut state, index, blank.as_bytes())?;
        }
        if index == 0 {
            let wire = fixtures::artifact(&mut state, true)?;
            state.pin_snapshot(fixtures::facts(&wire)?.0, SnapshotLimits::default())?;
        }
    }
    log.save_committed(f::at(2))?;
    if upgraded {
        state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    }
    drop((log, state));
    Ok(())
}

fn refusal(
    dir: &std::path::Path,
    cap: &CommittedRecoveryLimits,
    expected: CommittedRecoveryError,
    upgraded: bool,
) -> TestResult {
    let before = if upgraded {
        Some(fixtures::cohort(dir)?)
    } else {
        None
    };
    let legacy = if upgraded {
        None
    } else {
        Some(super::super::fixtures::cohort(dir)?)
    };
    let meta = f::log_meta(dir)?;
    let rows = f::log_rows(dir)?;
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let error = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *cap,
        snapshot()?,
    )
    .err()
    .ok_or("bounded recovery accepted")?;
    assert_eq!(error, Error::Recovery(expected));
    drop(f::open_pair(dir)?);
    if let Some(before) = before {
        assert_eq!(fixtures::cohort(dir)?, before);
    }
    if let Some(legacy) = legacy {
        assert_eq!(super::super::fixtures::cohort(dir)?, legacy);
    }
    assert_eq!(f::log_meta(dir)?, meta);
    assert_eq!(f::log_rows(dir)?, rows);
    Ok(())
}

#[test]
fn scalar_mode_dispatch_keeps_legacy_exact_workspace_and_pays_new_context_before_work() -> TestResult
{
    for upgraded in [false, true] {
        let dir = f::directory("advance-mode-workspace")?;
        seed(&dir, upgraded)?;
        let base = if upgraded { 4100 } else { 3588 };
        let exact = base + 2050 + f::MIB;
        if upgraded {
            refusal(
                &dir,
                &profile((3, 39), 525, exact - 1)?,
                CommittedRecoveryError::Limit,
                upgraded,
            )?;
        }
        for maximum in [exact, with_headers(exact)? - 1] {
            refusal(
                &dir,
                &profile((3, 39), 525, maximum)?,
                CommittedRecoveryError::Limit,
                upgraded,
            )?;
        }
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(state.supports_snapshot_transitions()?, upgraded);
        let scope = f::scope()?;
        let (log, mut state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            profile((3, 39), 525, with_headers(exact)?)?,
            snapshot()?,
        )?;
        assert_eq!(
            f::applied(&mut state)?
                .0
                .last_entry()
                .map(EntryIdentity::log),
            Some(f::at(2))
        );
        drop((log, state));
        drop(f::open_pair(&dir)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn one_actual_point_body_pool_covers_orphan_pin_audit_then_replay() -> TestResult {
    let dir = f::directory("advance-shared-work")?;
    seed(&dir, true)?;
    for (work, entry, error) in [
        ((2, 39), 525, CommittedRecoveryError::Limit),
        (
            (3, 38),
            525,
            CommittedRecoveryError::Log(RaftLogError::Limit),
        ),
        (
            (3, 39),
            524,
            CommittedRecoveryError::Log(RaftLogError::Limit),
        ),
    ] {
        refusal(&dir, &profile(work, entry, 4 * f::MIB)?, error, true)?;
    }
    let (log, state) = f::open_pair(&dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        profile((3, 39), 525, 4 * f::MIB)?,
        snapshot()?,
    )?;
    assert_eq!(log.state()?.last_purged(), None);
    assert_eq!(
        f::applied(&mut state)?
            .0
            .last_entry()
            .map(EntryIdentity::log),
        Some(f::at(2))
    );
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn new_mode_maximum_equal_anchor_requires_zero_copied_log_points() -> TestResult {
    let dir = f::directory("advance-max")?;
    let (log, state) = f::create_pair(&dir)?;
    drop((log, state));
    let blank = f::blank(EntryStamp::Unstamped)?;
    let p = f::at(u64::MAX);
    let control = ControlState::restored(
        f::clock(0, 0, 0)?,
        EntryIdentity::new(p, blank.digest()),
        EntryStamp::Unstamped,
    )?;
    super::super::fixtures::replace_control(&dir, control, false)?;
    let (mut log, mut state) = f::open_pair(&dir)?;
    let wire = fixtures::artifact(&mut state, true)?;
    let id = fixtures::facts(&wire)?.0;
    state.pin_snapshot(id, SnapshotLimits::default())?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    // Explicit trusted mathematical base, not fabricated local provenance.
    let before_log = log.state()?;
    log.install_snapshot_boundary(before_log, p, *blank.digest().as_bytes(), id)?;
    drop((log, state));
    let before = fixtures::cohort(&dir)?;
    let meta = f::log_meta(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        profile((1, 1), 512, 4 * f::MIB)?,
        snapshot()?,
    )?;
    assert_eq!(log.state()?.last_purged(), Some(p));
    assert_eq!(f::applied(&mut state)?.0, control);
    drop((log, state));
    assert_eq!(fixtures::cohort(&dir)?, before);
    assert_eq!(f::log_meta(&dir)?, meta);
    assert!(f::log_rows(&dir)?.is_empty());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn large_normal_scope_is_admitted_before_copies_even_with_tiny_driver_workspace() -> TestResult {
    let dir = f::directory("advance-large-scope")?;
    let scope = f::scope_for(&[255; 4096], b"g", b"z")?;
    let (mut log, mut state) = f::create_pair_for(&dir, &scope)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 0, blank.as_bytes())?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    log.save_committed(f::at(0))?;
    let wire = fixtures::artifact(&mut state, true)?;
    let id = fixtures::facts(&wire)?.0;
    state.pin_snapshot(id, SnapshotLimits::default())?;
    state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    log.purge_through(f::at(0), id)?;
    let mut operation = f::create_op();
    let Operation::Create(ref mut create) = operation else {
        return Err("not Create".into());
    };
    create.payload = Some(vec![0xa5; f::MIB]);
    let frame = f::frame_for(&scope, 5, b"a", 1, vec![(b"i", operation)])?;
    let length = frame.as_bytes().len();
    assert_eq!(length, 1_052_790);
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    let exact = 4100 + 512 + length + 4610;
    assert_eq!(exact, 1_062_012);
    let before = fixtures::cohort(&dir)?;
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    let mut normal = f::driver_limits();
    normal.workspace_bytes = 1;
    for (workspace, expected) in [
        (exact - 1, CommittedRecoveryError::Limit),
        (exact, CommittedRecoveryError::Limit),
        (with_headers(exact)? - 1, CommittedRecoveryError::Limit),
        (
            with_headers(exact)?,
            CommittedRecoveryError::Normal(
                wf_storage::transaction::RecordBatchTransactionError::Apply(
                    wf_ports::log::RecordBatchApplyError::Workspace {
                        attempted: 160 * f::MIB,
                        maximum: 1,
                    },
                ),
            ),
        ),
    ] {
        let cap = CommittedRecoveryLimits::new(
            RaftMembershipLimits::default(),
            (100, 1536, 1024, 131),
            normal,
            (1, length),
            512 + length,
            workspace,
        )?;
        let (log, state) = f::open_pair_for(&dir, &scope)?;
        assert_eq!(
            recover_snapshotted(
                log,
                state,
                scope.group(),
                scope.recovery(),
                cap,
                snapshot()?
            )
            .err()
            .ok_or("tiny Normal workspace accepted")?,
            Error::Recovery(expected)
        );
        assert_eq!(fixtures::cohort(&dir)?, before);
        assert_eq!(f::log_meta(&dir)?, meta);
        assert_eq!(f::log_rows(&dir)?, rows);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn with_headers(bytes: usize) -> TestResult<usize> {
    let fixed = std::mem::size_of::<Option<[usize; 4]>>()
        .checked_add(
            std::mem::size_of::<wf_node::recovery::CommittedRecoveryError>()
                .max(std::mem::size_of::<wf_node::recovery::SnapshotRecoveryError>()),
        )
        .ok_or("fixed recovery header overflow")?;
    Ok(bytes.checked_add(fixed).ok_or("outer profile overflow")?)
}
