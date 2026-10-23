//! Immutable atomic originals through real staging, install and purged startup.

use super::{
    CommittedRecoveryError, Error, RaftLogStore, SnapshotError, SnapshotLimits, StateSnapshotStore,
    TestResult, caps, f, fixtures, purge_snapshot, recover_snapshotted,
};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_ports::log::CodecLimits;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::StateStoreLimits;
use wf_storage::transaction::RecordBatchTransactionError;
use wf_types::identity::{PartitionId, RecordId};

pub(super) fn seed(dir: &Path, complete: bool) -> TestResult<Vec<u8>> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        16 * f::MIB,
        16 * f::MIB,
    )?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    assert!(
        tx.installed_policy(s.tenant(), b"u", schema, &mut charge)?
            .is_none()
    );
    drop(tx);
    let bootstrap = f::frame(5, b"b", 1, vec![(b"b", f::create_op())])?;
    f::append(&mut log, 0, bootstrap.as_bytes())?;
    drop(f::apply_normal(&mut state, 0, bootstrap.as_bytes(), 5)?);
    let atomic = f::frame(7, b"q", 17, vec![(b"i", f::atomic_start_op(b"u"))])?;
    f::append(&mut log, 1, atomic.as_bytes())?;
    drop(f::apply_normal(&mut state, 1, atomic.as_bytes(), 7)?);
    if complete {
        let frame = f::atomic_complete_frame(&mut state)?;
        f::append(&mut log, 2, frame.as_bytes())?;
        drop(f::apply_normal(&mut state, 2, frame.as_bytes(), 5)?);
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            16 * f::MIB,
            16 * f::MIB,
        )?;
        let current = tx
            .record(
                s.tenant(),
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
            )?
            .ok_or("completed base record absent")?;
        assert_eq!(current.record.state(), b"completed");
        assert_eq!(current.record.version(), 2);
        assert_eq!(current.record.result(), Some(b"done".as_slice()));
        assert!(current.record.lease().is_none());
    }
    log.save_committed(f::at(if complete { 2 } else { 1 }))?;
    drop(f::atomic_original(&mut state)?);
    let mut wire = Vec::new();
    let metadata = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(metadata.id().as_slice(), Sha256::digest(&wire).as_slice());
    assert_eq!(
        metadata.control().last_entry().map(EntryIdentity::log),
        Some(f::at(if complete { 2 } else { 1 }))
    );
    assert_eq!(fixtures::artifact(&mut state)?, wire);
    Ok(wire)
}

fn stage(path: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let s = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        s.group().clone(),
        s.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}

fn tail(dir: &Path, completed_base: bool, applied_suffix: bool) -> TestResult<Vec<u8>> {
    let wire = seed(dir, completed_base)?;
    let (mut log, mut state) = f::open_pair(dir)?;
    if !completed_base {
        let complete = f::atomic_complete_frame(&mut state)?;
        f::append(&mut log, 2, complete.as_bytes())?;
        if applied_suffix {
            drop(f::apply_normal(&mut state, 2, complete.as_bytes(), 5)?);
        }
    }
    let retry = f::frame(7, b"q", 18, vec![(b"i", f::atomic_start_op(b"u"))])?;
    f::append(&mut log, 3, retry.as_bytes())?;
    if applied_suffix {
        drop(f::apply_normal(&mut state, 3, retry.as_bytes(), 7)?);
    }
    let end = if completed_base { 3 } else { 4 };
    if !completed_base {
        let blank = f::blank(EntryStamp::Unstamped)?;
        f::append(&mut log, 4, blank.as_bytes())?;
        if applied_suffix {
            f::apply_control(&mut state, 4, blank.as_bytes())?;
        }
    }
    f::append(&mut log, end + 1, b"uncommitted atomic snapshot tail")?;
    log.save_committed(f::at(end))?;
    let s = f::scope()?;
    let (mut log, mut state) = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(
        log.state()?.last_purged(),
        Some(f::at(if completed_base { 2 } else { 1 }))
    );
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(&wire).into())
    );
    assert_eq!(fixtures::artifact(&mut state)?, wire);
    Ok(wire)
}

fn reframe(meta: &f::Rows, rows: &f::Rows) -> TestResult<Vec<u8>> {
    let length = 68
        + 12 * (meta.len() + rows.len())
        + meta
            .iter()
            .chain(rows)
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>();
    let mut out = b"FWSP\0\x01\0\x01g\0\x01z".to_vec();
    for count in [meta.len(), rows.len(), length] {
        out.extend_from_slice(&u64::try_from(count)?.to_be_bytes());
    }
    for (key, value) in meta.iter().chain(rows) {
        out.extend_from_slice(&u32::try_from(key.len())?.to_be_bytes());
        out.extend_from_slice(&u64::try_from(value.len())?.to_be_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    let digest = Sha256::digest(&out);
    out.extend_from_slice(&digest);
    assert_eq!(out.len(), length);
    Ok(out)
}

#[test]
fn atomic_start_snapshot_preserves_original_after_restore_and_install() -> TestResult {
    let source = f::directory("atomic-snapshot-source")?;
    let wire = seed(&source, false)?;
    let expected = fixtures::cohort(&source)?;
    assert_eq!(expected[1], expected[3]);
    let s = f::scope()?;
    for install in [false, true] {
        let dir = f::directory("atomic-snapshot-receiver")?;
        let mut state = if install {
            let (log, mut state) = f::create_pair(&dir)?;
            let candidate = stage(&dir.join("candidate.redb"), &wire)?;
            let metadata = state.install_snapshot(candidate, SnapshotLimits::default())?;
            assert_eq!(metadata.id().as_slice(), Sha256::digest(&wire).as_slice());
            drop(log);
            state
        } else {
            stage(&dir.join("state.redb"), &wire)?.into_store()?
        };
        let original = f::atomic_original(&mut state)?;
        assert_eq!(original.response().len(), 114);
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            16 * f::MIB,
            16 * f::MIB,
        )?;
        let row = tx
            .record(
                s.tenant(),
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
            )?
            .ok_or("restored atomic row absent")?;
        assert_eq!(row.record.state(), b"running");
        assert_eq!(row.record.run_state(), Some(b"q".as_slice()));
        assert_eq!(
            (
                row.record.version(),
                row.record.fence().get(),
                row.record.attempts()
            ),
            (1, 1, 1)
        );
        let lease = row.record.lease().ok_or("restored lease absent")?;
        assert_eq!(lease.worker.as_slice(), b"w\0\xff");
        assert_eq!(lease.token.as_bytes(), b"w\0\xff:17:1");
        assert_eq!(lease.deadline.get(), 18);
        assert_eq!(
            row.record
                .next_run_at()
                .map(wf_types::version::TimestampMillis::get),
            Some(18)
        );
        assert_eq!(
            (row.record.created_at().get(), row.record.updated_at().get()),
            (17, 17)
        );
        assert_eq!(
            row.record.latest_event_witness(),
            (
                wf_domain::history::EventKind::Created,
                1,
                row.record.fence()
            )
        );
        assert!(row.record.payload().is_none());
        assert!(row.record.result().is_none());
        assert!(row.record.error().is_none());
        assert_eq!(row.ordering.priority, 0);
        assert_eq!(row.ordering.state_enter_seq, 1_u128 << 32);
        drop(tx);
        assert_eq!(fixtures::artifact(&mut state)?, wire);
        drop(state);
        assert_eq!(fixtures::cohort(&dir)?, expected);
        if install {
            drop(f::open_pair(&dir)?);
            assert_eq!(fixtures::cohort(&dir)?, expected);
        }
        assert_eq!(fixtures::cohort(&source)?, expected);
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(source)?;
    Ok(())
}

#[test]
fn atomic_start_snapshot_rejects_rehashed_original_response_corruption() -> TestResult {
    let source = f::directory("atomic-snapshot-corruption")?;
    let wire = seed(&source, false)?;
    let original = fixtures::cohort(&source)?;
    for time in [false, true] {
        let mut rows = original[3].clone();
        let value = rows
            .iter_mut()
            .find(|(key, value)| {
                key.get(6) == Some(&2) && value.windows(4).any(|part| part == b"FWAS")
            })
            .ok_or("original atomic response row absent")?
            .1;
        if time {
            let mut offset = 6;
            for _ in 0..4 {
                let length = u32::from_be_bytes(value[offset..offset + 4].try_into()?);
                let len = usize::try_from(length)?;
                offset += 4 + len;
            }
            offset += 34 + 24;
            value[offset..offset + 8].copy_from_slice(&18_u64.to_be_bytes());
        } else {
            let token = value
                .windows(8)
                .position(|part| part == b"w\0\xff:17:1")
                .ok_or("original atomic token absent")?;
            value[token + 7] = b'2';
        }
        let changed = reframe(&original[2], &rows)?;
        assert_eq!(
            &changed[changed.len() - 32..],
            Sha256::digest(&changed[..changed.len() - 32]).as_slice()
        );
        let path = source.join(if time {
            "bad-time.redb"
        } else {
            "bad-token.redb"
        });
        let error = stage(&path, &changed)
            .err()
            .ok_or("rehashed semantic corruption accepted")?;
        assert_eq!(
            error.downcast_ref::<SnapshotError>(),
            Some(&SnapshotError::Stage(StateTransactionError::Corrupt))
        );
        drop(redb::Builder::new().open(&path)?);
        std::fs::remove_file(path)?;
        assert_eq!(fixtures::cohort(&source)?, original);
    }
    let mut valid = stage(&source.join("valid.redb"), &wire)?.into_store()?;
    drop(f::atomic_original(&mut valid)?);
    assert_eq!(fixtures::artifact(&mut valid)?, wire);
    drop(valid);
    assert_eq!(fixtures::cohort(&source)?, original);
    std::fs::remove_dir_all(source)?;
    Ok(())
}

#[test]
fn atomic_start_purged_startup_replays_completion_and_original() -> TestResult {
    let reference = f::directory("atomic-purged-reference")?;
    let wire = tail(&reference, false, true)?;
    let expected = fixtures::cohort(&reference)?;
    let logs = (f::log_rows(&reference)?, f::log_meta(&reference)?);
    let dir = f::directory("atomic-purged-replay")?;
    assert_eq!(tail(&dir, false, false)?, wire);
    let s = f::scope()?;
    for _ in 0..2 {
        let (log, state) = f::open_pair(&dir)?;
        let (mut log, mut state) = recover_snapshotted(
            log,
            state,
            s.group(),
            s.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        drop(f::atomic_original(&mut state)?);
        assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
        assert_eq!(log.state()?.committed(), Some(f::at(4)));
        assert_eq!(log.state()?.last_log(), Some(f::at(5)));
        assert_eq!(fixtures::artifact(&mut state)?, wire);
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    }
    let narrow = f::directory("atomic-purged-original-cap")?;
    let completed_wire = tail(&narrow, true, false)?;
    let before = fixtures::cohort(&narrow)?;
    let log_before = (f::log_rows(&narrow)?, f::log_meta(&narrow)?);
    for inline in [7, 8] {
        let mut normal = f::driver_limits();
        normal.codec = CodecLimits::new(AdmissionLimits::new(inline, 16 * f::MIB, 1000)?);
        let cap = super::CommittedRecoveryLimits::new(
            wf_ports::consensus::RaftMembershipLimits::default(),
            (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
            normal,
            (1000, 64 * f::MIB),
            16 * f::MIB,
            512 * f::MIB,
        )?;
        if inline == 7 {
            fixtures::assert_refusal(
                &narrow,
                false,
                cap,
                SnapshotLimits::default(),
                Error::Recovery(CommittedRecoveryError::Normal(
                    RecordBatchTransactionError::Transaction(StateTransactionError::Limit),
                )),
            )?;
            assert_eq!(fixtures::cohort(&narrow)?, before);
            assert_eq!((f::log_rows(&narrow)?, f::log_meta(&narrow)?), log_before);
        } else {
            let (log, state) = f::open_pair(&narrow)?;
            let (log, mut state) = recover_snapshotted(
                log,
                state,
                s.group(),
                s.recovery(),
                cap,
                SnapshotLimits::default(),
            )?;
            drop(f::atomic_original(&mut state)?);
            assert_eq!(
                f::applied(&mut state)?
                    .0
                    .last_entry()
                    .map(EntryIdentity::log),
                Some(f::at(3))
            );
            assert_eq!(fixtures::artifact(&mut state)?, completed_wire);
            drop((log, state));
            assert_eq!((f::log_rows(&narrow)?, f::log_meta(&narrow)?), log_before);
        }
    }
    for dir in [narrow, dir, reference] {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
