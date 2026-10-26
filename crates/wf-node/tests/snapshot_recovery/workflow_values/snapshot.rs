//! FWSP4 custody and real operation-eight suffixes on local and received bases.

use std::io::Cursor;
use std::path::Path;

use sha2::{Digest, Sha256};
use wf_domain::workflow::{Operation, StartAndClaim};
use wf_ports::blob::{OriginalValueRefKind, WorkflowValueReadBatch};
use wf_ports::consensus::WorkflowValuePolicyWitness;
use wf_ports::log::{PreparedBytes, RaftLogStore};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};

use super::{TestResult, f, fixtures as v, recover_snapshotted};
use wf_node::recovery::{install_received_snapshot, purge_snapshot, restore_received_snapshot};

#[test]
fn value_snapshot_recovery_preserves_catalog_for_all_contained_modes() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    for mode in [1, 5, 6, 7] {
        snapshot_case(mode, &limits)?;
    }
    Ok(())
}

fn snapshot_case(mode: u8, limits: &super::Limits) -> TestResult {
    let dir = f::directory("node-value-snapshot-matrix")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let original = mode != 6;
    let (wire, policy) = seed(&source, original)?;
    let suffix = suffix_frame(mode, &policy)?;
    append_tail(&source, &suffix)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&source)?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    assert_eq!(log.state()?.last_log(), Some(f::at(3)));
    let read = verify_values(&state, mode, &policy)?;
    drop((log, state));
    assert_values(&read, mode);
    let expected = v::cohort(&source)?;
    for route in ["matching", "fresh"] {
        let target = dir.join(route);
        std::fs::create_dir(&target)?;
        received(&target, route, &wire, &suffix, &policy, original, limits)
            .map_err(|error| format!("value snapshot mode {mode}, route {route}: {error:?}"))?;
        finish(&target, &wire, mode, &policy, limits)?;
        let observed = v::cohort(&target)?;
        assert_eq!(observed[1], expected[1]);
        assert_eq!(
            observed[0][b"manifest".as_slice()],
            expected[0][b"manifest".as_slice()]
        );
        assert_eq!(
            observed[0][b"origin".as_slice()],
            expected[0][b"origin".as_slice()]
        );
    }
    finish(&source, &wire, mode, &policy, limits)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn seed(dir: &Path, original: bool) -> TestResult<(Vec<u8>, WorkflowValuePolicyWitness)> {
    let (mut log, mut state) = v::pair(dir)?;
    let policy = v::install(&mut log, &mut state, original, None, 0, 10)?;
    let base = base_frame(&policy)?;
    f::append(&mut log, 1, base.as_bytes())?;
    v::accepted(v::apply(&mut state, &base, 1)?, 1, 100, 5)?;
    log.save_committed(f::at(1))?;
    let mut wire = Vec::new();
    let captured = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(&wire[..6], b"FWSP\0\x04");
    assert_eq!(captured.id().as_slice(), Sha256::digest(&wire).as_slice());
    assert_eq!(
        captured
            .control()
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(f::at(1))
    );
    let row_count = captured.row_count();
    drop((captured, state, log, base));
    let cohort = v::cohort(dir)?;
    assert_eq!(row_count, cohort[1].len() + 3);
    assert_eq!(u64::from_be_bytes(wire[12..20].try_into()?), 3);
    let manifest = &cohort[0][b"manifest".as_slice()];
    assert_eq!(manifest.len(), 210);
    assert_eq!(&manifest[118..178], &v::defaults(7_000).to_bytes()?);
    for slot in [2, 4] {
        if !cohort[slot].is_empty() {
            assert_eq!(
                cohort[slot].keys().map(Vec::as_slice).collect::<Vec<_>>(),
                [b"manifest".as_slice(), b"membership", b"origin"]
            );
        }
    }
    Ok((wire, policy))
}

fn base_frame(policy: &WorkflowValuePolicyWitness) -> TestResult<PreparedBytes> {
    v::encode(&v::batch(
        &f::scope()?,
        5,
        b"base",
        100,
        vec![v::create(b"base", 100, b"FSV2base")?],
        v::copy_policy(policy),
    )?)
}

fn suffix_frame(mode: u8, policy: &WorkflowValuePolicyWitness) -> TestResult<PreparedBytes> {
    let item = if mode == 1 {
        v::item(b"base", 200, f::cancel_op())?
    } else if mode == 7 {
        v::item(
            b"i",
            200,
            Operation::StartAndClaim(StartAndClaim {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                worker: b"worker".to_vec(),
                lease_ms: 5_000,
                payload: Some(b"value".to_vec()),
            }),
        )?
    } else {
        v::create(b"i", 200, b"value")?
    };
    v::encode(&v::batch(
        &f::scope()?,
        mode,
        b"suffix",
        200,
        vec![item],
        v::copy_policy(policy),
    )?)
}

fn append_tail(dir: &Path, suffix: &PreparedBytes) -> TestResult {
    let (mut log, state) = f::open_pair(dir)?;
    f::append(&mut log, 2, suffix.as_bytes())?;
    f::append(&mut log, 3, b"uncommitted opaque tail")?;
    log.save_committed(f::at(2))?;
    drop((log, state));
    Ok(())
}

fn stage(path: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let scope = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}

fn received(
    dir: &Path,
    route: &str,
    wire: &[u8],
    suffix: &PreparedBytes,
    policy: &WorkflowValuePolicyWitness,
    original: bool,
    limits: &super::Limits,
) -> TestResult {
    let scope = f::scope()?;
    if route == "matching" {
        let (local, _) = seed(dir, original)?;
        assert_eq!(local, wire);
        append_tail(dir, suffix)?;
        let candidate = stage(&dir.join("incoming.redb"), wire)?;
        let (log, state) = f::open_pair(dir)?;
        drop(install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            *limits,
            SnapshotLimits::default(),
        )?);
    } else if route == "fresh" {
        let mut log = v::log(dir)?;
        let policy_frame = v::policy_frame(original, None, 0, 10)?;
        let base = base_frame(policy)?;
        f::append(&mut log, 0, policy_frame.as_bytes())?;
        f::append(&mut log, 1, base.as_bytes())?;
        f::append(&mut log, 2, suffix.as_bytes())?;
        f::append(&mut log, 3, b"uncommitted opaque tail")?;
        log.save_committed(f::at(2))?;
        let candidate = stage(&dir.join("state.redb"), wire)?;
        drop(restore_received_snapshot(
            log,
            candidate,
            scope.group(),
            scope.recovery(),
            *limits,
            SnapshotLimits::default(),
        )?);
    } else {
        return Err("unknown received route".into());
    }
    drop(f::open_pair(dir)?);
    Ok(())
}

fn finish(
    dir: &Path,
    wire: &[u8],
    mode: u8,
    policy: &WorkflowValuePolicyWitness,
    limits: &super::Limits,
) -> TestResult {
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.committed(), Some(f::at(2)));
    assert_eq!(log.state()?.last_log(), Some(f::at(3)));
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    assert_eq!(
        v::control(&mut state)?
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(f::at(2))
    );
    let mut original = Vec::new();
    state
        .current_snapshot(&mut original, SnapshotLimits::default())?
        .ok_or("current FWSP4 missing")?;
    assert_eq!(original, wire);
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(wire).into())
    );
    let read = verify_values(&state, mode, policy)?;
    drop((log, state, original));
    assert_values(&read, mode);
    let before = v::cohort(dir)?;
    for _ in 0..2 {
        drop(f::open_pair(dir)?);
        assert_eq!(v::cohort(dir)?, before);
    }
    Ok(())
}

fn verify_values(
    state: &RedbStateStore,
    mode: u8,
    policy: &WorkflowValuePolicyWitness,
) -> TestResult<WorkflowValueReadBatch> {
    v::binding(
        state,
        b"base",
        Some("100-1"),
        (1_u128 << 32, [Some(1), None, None], None),
        policy,
    )?;
    let base = v::reference(b"base", OriginalValueRefKind::Payload, 1)?;
    if mode == 1 {
        let versions = [Some(1), None, Some(2)];
        v::binding(
            state,
            b"base",
            None,
            (1_u128 << 32, versions, Some(22_200)),
            policy,
        )?;
        v::binding(
            state,
            b"base",
            Some("200-2"),
            (1_u128 << 32, versions, Some(22_200)),
            policy,
        )?;
        let error = v::reference(b"base", OriginalValueRefKind::Error, 2)?;
        v::read(state, &[&base, &error, &base])
    } else {
        v::binding(
            state,
            b"base",
            None,
            (1_u128 << 32, [Some(1), None, None], None),
            policy,
        )?;
        v::binding(
            state,
            b"i",
            None,
            (2_u128 << 32, [Some(1), None, None], None),
            policy,
        )?;
        v::binding(
            state,
            b"i",
            Some("200-1"),
            (2_u128 << 32, [Some(1), None, None], None),
            policy,
        )?;
        let payload = v::reference(b"i", OriginalValueRefKind::Payload, 1)?;
        v::read(state, &[&base, &payload, &base])
    }
}

fn assert_values(read: &WorkflowValueReadBatch, mode: u8) {
    v::assert_read(
        read,
        &[
            b"FSV2\x01FSV2base",
            if mode == 1 {
                b"cancelled".as_slice()
            } else {
                b"value"
            },
        ],
        &[Some(0), Some(1), Some(0)],
    );
}
