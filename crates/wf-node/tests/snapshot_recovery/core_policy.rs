//! Mixed legacy/policy snapshots retain real origins, replies and committed suffixes.

use super::{
    CommittedRecoveryLimits, SnapshotLimits, StateSnapshotStore, TestResult, disk_backend, f,
    purge_snapshot, recover_snapshotted,
};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_node::recovery::install_received_snapshot;
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::RaftLogStore;
use wf_ports::snapshot::StateSnapshotTransitionStore;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction,
};
use wf_types::identity::RequestId;

fn normal() -> RecordBatchTransactionLimits {
    let mut value = f::driver_limits();
    value.workspace_bytes = 512 * f::MIB;
    value
}
fn recovery_caps() -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal(),
        (1_000, 64 * f::MIB),
        16 * f::MIB,
        1024 * f::MIB,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default())
    .with_core_policy_patches())
}
fn raw(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn policy() -> TestResult<Vec<u8>> {
    let mut intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
    raw(&mut intent, b"t")?;
    intent.push(1);
    intent.extend_from_slice(&7_u64.to_be_bytes());
    raw(&mut intent, b"r")?;
    intent.extend_from_slice(&[0; 4]);
    intent.extend_from_slice(&1_u32.to_be_bytes());
    raw(&mut intent, b"q")?;
    intent.extend_from_slice(&[1, 1, 0, 0, 0]);
    let mut out = b"FWCP\0\x01\x05".to_vec();
    for bytes in [b"t".as_slice(), b"g", b"z", b"policy"] {
        raw(&mut out, bytes)?;
    }
    out.extend_from_slice(&2_u16.to_be_bytes());
    out.extend_from_slice(&Sha256::digest(&intent));
    out.extend_from_slice(&11_u64.to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.extend_from_slice(&11_u64.to_be_bytes());
    raw(&mut out, &intent)?;
    Ok(out)
}
fn cohort(dir: &Path) -> TestResult<[Option<f::Rows>; 6]> {
    let tables = [
        "wf_state_meta_v1",
        "wf_state_rows_v1",
        "wf_state_snapshot_meta_v1",
        "wf_state_snapshot_rows_v1",
        "wf_state_snapshot_alt_meta_v1",
        "wf_state_snapshot_alt_rows_v1",
    ];
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let read = database.begin_read()?;
    let mut names = read
        .list_tables()?
        .map(|t| t.name().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = tables[..if names.len() == 4 { 4 } else { 6 }]
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = std::array::from_fn(|_| None);
    for (slot, name) in result.iter_mut().zip(tables) {
        if !names.iter().any(|n| n == name) {
            continue;
        }
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = f::Rows::new();
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
        *slot = Some(rows);
    }
    Ok(result)
}
fn artifact(state: &mut RedbStateStore, id: [u8; 32]) -> TestResult<Vec<u8>> {
    let mut wire = Vec::new();
    let metadata = state
        .snapshot_by_id(id, &mut wire, SnapshotLimits::default())?
        .ok_or("original snapshot ID missing")?;
    assert_eq!(metadata.id(), &id);
    assert_eq!(metadata.encoded_bytes(), wire.len());
    assert_eq!(id.as_slice(), Sha256::digest(&wire).as_slice());
    Ok(wire)
}
fn seed(dir: &Path, complete: bool) -> TestResult<(Vec<u8>, Vec<u8>)> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let member = f::member(EntryStamp::Unstamped, 4)?;
    f::append(&mut log, 0, member.as_bytes())?;
    f::apply_control(&mut state, 0, member.as_bytes())?;
    log.save_committed(f::at(0))?;
    let mut old = Vec::new();
    let old_id = *state
        .capture_snapshot(&mut old, SnapshotLimits::default())?
        .id();
    assert_eq!(&old[..6], b"FWSP\0\x01");
    let scope = f::scope()?;
    (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        recovery_caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old_id));
    let create = f::frame(5, b"create", 10, vec![(b"i", f::create_op())])?;
    f::append(&mut log, 1, create.as_bytes())?;
    drop(f::apply_normal(&mut state, 1, create.as_bytes(), 5)?);
    let policy = policy()?;
    f::append(&mut log, 2, &policy)?;
    let outcome = apply_complete_core_policy_patch_transaction(
        &mut state,
        &scope,
        &policy,
        f::at(2),
        normal(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
        return Err("missing real snapshot policy publication".into());
    };
    assert_eq!(
        (outcome.intent().version(), outcome.original_position()),
        (2, f::at(2))
    );
    assert_eq!(
        (outcome.original_time().get(), outcome.expires_at().get()),
        (11, 1011)
    );
    log.save_committed(f::at(2))?;
    let mut wire = Vec::new();
    let id = *state
        .capture_snapshot(&mut wire, SnapshotLimits::default())?
        .id();
    assert_eq!(&wire[..6], b"FWSP\0\x02");
    assert_ne!(id, old_id);
    assert_eq!(artifact(&mut state, old_id)?, old);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old_id));
    let cancel = f::frame(1, b"cancel", 13, vec![(b"i", f::cancel_op())])?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 3, cancel.as_bytes())?;
    f::append(&mut log, 4, blank.as_bytes())?;
    if complete {
        drop(f::apply_normal(&mut state, 3, cancel.as_bytes(), 1)?);
        f::apply_control(&mut state, 4, blank.as_bytes())?;
    }
    f::append(&mut log, 5, b"uncommitted policy snapshot tail")?;
    log.save_committed(f::at(4))?;
    Ok((old, wire))
}
fn inspect(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let schema = PolicySchemaLimits::default();
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let current = tx
        .installed_policy(
            scope.tenant(),
            b"r",
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?
        .ok_or("missing restored current full policy")?;
    assert_eq!(current.generation(), 8);
    let original = tx
        .outcome(scope.tenant(), &RequestId::new(b"policy".to_vec())?)?
        .ok_or("missing restored original policy outcome")?;
    assert_eq!(original.response(), current.encoded());
    assert_eq!(
        (
            original.original_position(),
            original.original_time().get(),
            original.expires_at().get()
        ),
        (f::at(2), 11, 1011)
    );
    assert_eq!(original.intent().version(), 2);
    Ok(())
}

#[test]
fn policy_snapshot_restart_preserves_old_anchor_and_committed_suffix() -> TestResult {
    let reference = f::directory("policy-snapshot-reference")?;
    let (old, wire) = seed(&reference, true)?;
    let expected = cohort(&reference)?;
    assert!(expected.iter().all(Option::is_some));
    let dir = f::directory("policy-snapshot-replay")?;
    assert_eq!(seed(&dir, false)?, (old.clone(), wire.clone()));
    let id: [u8; 32] = Sha256::digest(&wire).into();
    let old_id: [u8; 32] = Sha256::digest(&old).into();
    let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
    let scope = f::scope()?;
    for _ in 0..2 {
        let (log, state) = f::open_pair(&dir)?;
        let (mut log, mut state) = recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            recovery_caps()?,
            SnapshotLimits::default(),
        )?;
        inspect(&mut state)?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(0)));
        assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(old_id));
        assert_eq!(artifact(&mut state, old_id)?, old);
        assert_eq!(artifact(&mut state, id)?, wire);
        assert_eq!(
            f::applied(&mut state)?
                .0
                .last_entry()
                .map(EntryIdentity::log),
            Some(f::at(4))
        );
        drop((log, state));
        assert_eq!(cohort(&dir)?, expected);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    }
    advance(&reference, id, &wire)?;
    advance(&dir, id, &wire)?;
    assert_eq!(cohort(&dir)?, cohort(&reference)?);
    assert_eq!(
        (f::log_rows(&dir)?, f::log_meta(&dir)?),
        (f::log_rows(&reference)?, f::log_meta(&reference)?)
    );
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}
fn advance(dir: &Path, id: [u8; 32], wire: &[u8]) -> TestResult {
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        recovery_caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(2)));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    assert_eq!(artifact(&mut state, id)?, wire);
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    let control = f::applied(&mut state)?.0;
    assert_eq!(control.last_entry().map(EntryIdentity::log), Some(f::at(4)));
    drop((log, state));
    drop(f::open_pair(dir)?);
    Ok(())
}
fn stage(dir: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
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
fn install(dir: &Path, wire: &[u8]) -> TestResult<(wf_storage::log::RedbLogStore, RedbStateStore)> {
    let candidate = stage(dir, wire)?;
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    Ok(install_received_snapshot(
        log,
        state,
        candidate,
        scope.group(),
        scope.recovery(),
        recovery_caps()?,
        SnapshotLimits::default(),
    )?)
}
fn check_received(dir: &Path, wire: &[u8]) -> TestResult {
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        recovery_caps()?,
        SnapshotLimits::default(),
    )?;
    let id: [u8; 32] = Sha256::digest(wire).into();
    inspect(&mut state)?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(2)));
    assert_eq!(log.state()?.committed(), Some(f::at(2)));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    assert_eq!(artifact(&mut state, id)?, wire);
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    Ok(())
}
fn acknowledged_kill(child: &mut Child) -> TestResult {
    let result = (|| -> TestResult {
        let stdout = child
            .stdout
            .take()
            .ok_or("policy snapshot stdout missing")?;
        let mut stdin = child.stdin.take().ok_or("policy snapshot stdin missing")?;
        let (sender, receiver) = mpsc::sync_channel(2);
        let reader = disk_backend::signal_reader(stdout, sender, true)?;
        let acknowledgement = (|| -> TestResult {
            receiver.recv_timeout(Duration::from_secs(20))??;
            stdin.write_all(b"A")?;
            stdin.flush()?;
            receiver.recv_timeout(Duration::from_secs(20))??;
            Ok(())
        })();
        let exit = disk_backend::terminate(child);
        drop(stdin);
        let joined = reader.join();
        acknowledgement?;
        assert!(!exit?.success());
        joined.map_err(|_| "policy snapshot reader panicked")??;
        Ok(())
    })();
    if result.is_err() {
        let _ = disk_backend::terminate(child);
    }
    result
}

#[test]
fn policy_snapshot_receive_purge_and_crash_reopen_keep_exact_generations() -> TestResult {
    if let Some(dir) = std::env::var_os("WF_POLICY_SNAPSHOT_DIR") {
        let dir = Path::new(&dir);
        let wire = std::fs::read(dir.join("wire.bin"))?;
        let _owners = install(dir, &wire)?;
        disk_backend::parked("after-return")?;
        return Err("child missed acknowledged snapshot boundary".into());
    }
    let source = f::directory("policy-received-source")?;
    let (_, wire) = seed(&source, false)?;
    let reference = f::directory("policy-received-reference")?;
    drop(f::create_pair(&reference)?);
    let (log, mut state) = install(&reference, &wire)?;
    inspect(&mut state)?;
    let id: [u8; 32] = Sha256::digest(&wire).into();
    assert_eq!(artifact(&mut state, id)?, wire);
    drop((log, state));
    check_received(&reference, &wire)?;
    let expected = cohort(&reference)?;
    assert!(expected.iter().all(Option::is_some));
    let logs = (f::log_rows(&reference)?, f::log_meta(&reference)?);
    let dir = f::directory("policy-received-kill")?;
    drop(f::create_pair(&dir)?);
    std::fs::write(dir.join("wire.bin"), &wire)?;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "core_policy::policy_snapshot_receive_purge_and_crash_reopen_keep_exact_generations",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WF_POLICY_SNAPSHOT_DIR", &dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    acknowledged_kill(&mut child)?;
    assert_eq!(cohort(&dir)?, expected);
    assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    check_received(&dir, &wire)?;
    assert_eq!(cohort(&dir)?, expected);
    drop(f::open_pair(&dir)?);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(source)?;
    Ok(())
}
