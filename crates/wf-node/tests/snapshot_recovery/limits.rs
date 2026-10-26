//! Snapshot phases and the shared retained-log point/body pool stay prospective.

use super::{
    CommittedRecoveryError, CommittedRecoveryLimits, Error, RaftLogStore, SnapshotLimits,
    StateSnapshotStore, TestResult, caps, f, fixtures, purge_snapshot, recover_snapshotted,
};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::{consensus::RaftMembershipLimits, log::RaftLogError};

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

fn blank_pair(dir: &std::path::Path) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    assert_eq!(blank.as_bytes().len(), 13);
    for index in 0..=2 {
        f::append(&mut log, index, blank.as_bytes())?;
        if index < 2 {
            f::apply_control(&mut state, index, blank.as_bytes())?;
        }
        if index == 0 {
            fixtures::capture(&mut state)?;
        }
    }
    log.save_committed(f::at(2))?;
    drop((log, state));
    Ok(())
}

#[test]
fn snapshot_phase_workspace_is_admitted_before_other_owners_and_exact_profile_succeeds()
-> TestResult {
    let dir = f::directory("snapshot-phase-budget")?;
    blank_pair(&dir)?;
    let cap = snapshot()?;
    let exact = 3588 + 2050 + f::MIB;
    for outer in [exact - 1, exact, with_headers(exact)? - 1] {
        fixtures::assert_refusal(
            &dir,
            false,
            &profile((3, 39), 525, outer)?,
            cap,
            Error::Recovery(CommittedRecoveryError::Limit),
        )?;
    }
    let (log, state) = f::open_pair(&dir)?;
    let s = f::scope()?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        profile((3, 39), 525, with_headers(exact)?)?,
        cap,
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
    Ok(())
}

#[test]
fn one_point_body_pool_counts_audit_and_replay_and_excludes_only_purged_points() -> TestResult {
    for purged in [false, true] {
        let dir = f::directory("snapshot-point-budget")?;
        blank_pair(&dir)?;
        let s = f::scope()?;
        if purged {
            let (log, state) = f::open_pair(&dir)?;
            drop(purge_snapshot(
                log,
                state,
                s.group(),
                s.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )?);
        }
        let n = if purged { 2 } else { 3 };
        for (work, entry, expected) in [
            ((n - 1, 13 * n), 525, CommittedRecoveryError::Limit),
            (
                (n, 13 * n - 1),
                525,
                CommittedRecoveryError::Log(RaftLogError::Limit),
            ),
            (
                (n, 13 * n),
                524,
                CommittedRecoveryError::Log(RaftLogError::Limit),
            ),
        ] {
            fixtures::assert_refusal(
                &dir,
                false,
                &profile(work, entry, 4 * f::MIB)?,
                snapshot()?,
                Error::Recovery(expected),
            )?;
        }
        let (log, state) = f::open_pair(&dir)?;
        let (log, mut state) = recover_snapshotted(
            log,
            state,
            s.group(),
            s.recovery(),
            profile((n, 13 * n), 525, 4 * f::MIB)?,
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
fn maximum_equal_applied_committed_and_purged_boundary_requires_zero_log_points() -> TestResult {
    let dir = f::directory("snapshot-max-boundary")?;
    let (log, state) = f::create_pair(&dir)?;
    drop((log, state));
    let blank = f::blank(EntryStamp::Unstamped)?;
    let p = f::at(u64::MAX);
    let control = ControlState::restored(
        f::clock(0, 0, 0)?,
        EntryIdentity::new(p, blank.digest()),
        EntryStamp::Unstamped,
    )?;
    fixtures::replace_control(&dir, control, false)?;
    let (log, mut state) = f::open_pair(&dir)?;
    let id = fixtures::capture(&mut state)?;
    state.pin_snapshot(id, SnapshotLimits::default())?;
    drop((log, state));
    install_trusted_max_anchor(&dir, id)?;
    let before = fixtures::cohort(&dir)?;
    let meta = f::log_meta(&dir)?;
    assert!(f::log_rows(&dir)?.is_empty());
    let (log, state) = f::open_pair(&dir)?;
    let s = f::scope()?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        profile((1, 1), 512, 4 * f::MIB)?,
        snapshot()?,
    )?;
    assert_eq!(log.state()?.last_purged(), Some(p));
    assert_eq!(log.state()?.committed(), Some(p));
    assert_eq!(log.state()?.last_log(), Some(p));
    assert_eq!(f::applied(&mut state)?.0, control);
    drop((log, state));
    assert_eq!(fixtures::cohort(&dir)?, before);
    assert_eq!(f::log_meta(&dir)?, meta);
    assert!(f::log_rows(&dir)?.is_empty());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn install_trusted_max_anchor(dir: &std::path::Path, id: [u8; 32]) -> TestResult {
    use sha2::{Digest, Sha256};
    // Preexisting trusted boundary fixture, not a local-history or received-
    // snapshot provenance proof. The production coordinator never writes this.
    let mut wire = b"FWLS\0\x02\x0e".to_vec();
    for _ in 0..3 {
        wire.extend_from_slice(&1_u64.to_be_bytes());
        wire.extend_from_slice(&7_u64.to_be_bytes());
        wire.extend_from_slice(&u64::MAX.to_be_bytes());
    }
    wire.extend_from_slice(&id);
    let mut hash = Sha256::new();
    hash.update(b"wf-raft-log-v1\0\x01g\0\x01z");
    hash.update(&wire);
    wire.extend_from_slice(&hash.finalize());
    let database = redb::Builder::new().open(dir.join("log.redb"))?;
    let mut tx = database.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    {
        let mut meta = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_raft_meta_v1",
        ))?;
        assert!(meta.insert(b"state".as_slice(), wire.as_slice())?.is_some());
    }
    tx.commit()?;
    Ok(())
}

#[test]
fn external_large_tenant_is_admitted_before_scope_copy_with_tiny_driver_workspace() -> TestResult {
    let dir = f::directory("snapshot-large-scope")?;
    let s = f::scope_for(&[255; 4096], b"g", b"z")?;
    let (mut log, mut state) = f::create_pair_for(&dir, &s)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 0, blank.as_bytes())?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    fixtures::capture(&mut state)?;
    let mut operation = f::create_op();
    let wf_domain::workflow::Operation::Create(ref mut create) = operation else {
        return Err("fixture is not Create".into());
    };
    create.payload = Some(vec![0xa5; f::MIB]);
    let frame = f::frame_for(&s, 5, b"a", 1, vec![(b"i", operation)])?;
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop(purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?);
    let length = frame.as_bytes().len();
    assert_eq!(length, f::MIB + 4214);
    let q = 512 + 4096 + 1 + 1;
    let exact = 3588 + 512 + length + q;
    assert_eq!(exact, 1_061_500);
    assert!(
        exact > 3588 + snapshot()?.maximum_metadata_bytes() + snapshot()?.maximum_temporary_bytes(),
        "the separate snapshot phase must not mask Q-before-copy"
    );
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
        fixtures::assert_refusal(&dir, false, &cap, snapshot()?, Error::Recovery(expected))?;
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
