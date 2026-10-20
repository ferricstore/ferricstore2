//! Complete Claim receipts survive immutable capture, install and purged replay.

use super::{RaftLogStore, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult};
use super::{caps, f, fixtures, purge_snapshot, recover_snapshotted};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryStamp;
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::{Create, Operation};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::log::RedbLogStore;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{
    ClaimTransactionLimits, ClaimTransactionOutcome, apply_claim_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};

fn raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

fn phase(now: u64, kind: &[u8]) -> TestResult<Vec<u8>> {
    let mut out = b"FWCP\0\x01\x03".to_vec();
    for value in [b"t".as_slice(), b"g", b"z", b"claim"] {
        raw(&mut out, value)?;
    }
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&[7; 32]);
    out.extend_from_slice(&now.to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.extend_from_slice(&now.to_be_bytes());
    raw(&mut out, kind)?;
    out.extend_from_slice(&[0; 7]);
    raw(&mut out, b"w")?;
    out.extend_from_slice(&50_u64.to_be_bytes());
    for count in [1_u32, 3, 4] {
        out.extend_from_slice(&count.to_be_bytes());
    }
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&u32::MAX.to_be_bytes());
    Ok(out)
}

fn receipt() -> TestResult<Vec<u8>> {
    let mut out = b"FWCR\0\x01\0\0\0".to_vec();
    for count in [1_u32, 0, 1] {
        out.extend_from_slice(&count.to_be_bytes());
    }
    for value in [b"p".as_slice(), b"i", b"c", b"q", b"w", b"w:11:1"] {
        raw(&mut out, value)?;
    }
    for number in [2_u64, 1, 61, 10, 11, 0] {
        out.extend_from_slice(&number.to_be_bytes());
    }
    out.push(0);
    out.extend_from_slice(&(1_u128 << 32).to_be_bytes());
    out.push(1);
    raw(&mut out, &[0, 255])?;
    out.extend_from_slice(&[0, 0]);
    Ok(out)
}

fn apply(state: &mut RedbStateStore, index: u64, body: &[u8]) -> TestResult {
    let result = apply_claim_transaction(
        state,
        &f::scope()?,
        body,
        f::at(index),
        ClaimTransactionLimits {
            base: f::driver_limits(),
            work: ClaimWorkLimits::default(),
        },
    )?;
    let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("original snapshot Claim receipt missing".into());
    };
    assert_eq!(outcome.response(), receipt()?);
    assert_eq!(outcome.response_version(), 3);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), f::at(2));
    assert_eq!(outcome.original_time().get(), 11);
    assert_eq!(outcome.expires_at().get(), 1011);
    Ok(())
}

fn seed(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, Vec<u8>)> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let member = f::member(EntryStamp::Unstamped, 4)?;
    f::append(&mut log, 0, member.as_bytes())?;
    f::apply_control(&mut state, 0, member.as_bytes())?;
    let create = Operation::Create(Create {
        workflow_type: b"c".to_vec(),
        state: b"q".to_vec(),
        run_at: Some(wf_types::version::TimestampMillis::new(10)),
        payload: Some(vec![0, 255]),
    });
    let frame = f::frame(5, b"create", 10, vec![(b"i", create)])?;
    f::append(&mut log, 1, frame.as_bytes())?;
    let _ = f::apply_normal(&mut state, 1, frame.as_bytes(), 5)?;
    let body = phase(11, b"c")?;
    f::append(&mut log, 2, &body)?;
    apply(&mut state, 2, &body)?;
    log.save_committed(f::at(2))?;
    let mut wire = Vec::new();
    let metadata = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(metadata.id().as_slice(), Sha256::digest(&wire).as_slice());
    assert_eq!(
        metadata.control().last_entry().map(|entry| entry.log()),
        Some(f::at(2))
    );
    assert_eq!(
        metadata.membership().entry().map(|entry| entry.log()),
        Some(f::at(0))
    );
    Ok((log, state, wire))
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

fn inspect(state: &mut RedbStateStore) -> TestResult {
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        16 * f::MIB,
        16 * f::MIB,
    )?;
    let original = tx
        .outcome(s.tenant(), &RequestId::new(b"claim".to_vec())?)?
        .ok_or("restored Claim outcome missing")?;
    assert_eq!(original.response(), receipt()?);
    assert_eq!(original.original_position(), f::at(2));
    assert_eq!(original.original_time().get(), 11);
    assert_eq!(original.expires_at().get(), 1011);
    let record = tx
        .record(
            s.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("restored leased record missing")?;
    assert_eq!(record.record.state(), b"running");
    assert_eq!(record.record.run_state(), Some(b"q".as_slice()));
    assert_eq!(
        (
            record.record.version(),
            record.record.fence().get(),
            record.record.attempts()
        ),
        (2, 1, 0)
    );
    let lease = record.record.lease().ok_or("restored lease missing")?;
    assert_eq!(
        (
            lease.worker.as_slice(),
            lease.token.as_bytes(),
            lease.deadline.get()
        ),
        (b"w".as_slice(), b"w:11:1".as_slice(), 61)
    );
    assert_eq!(record.ordering.state_enter_seq, 1_u128 << 32);
    Ok(())
}

fn due_key() -> Vec<u8> {
    let mut out = b"FWPK\0\x01\x03g\0\0z\0\0t\0\0\x02".to_vec();
    out.extend_from_slice(&61_u64.to_be_bytes());
    out.extend_from_slice(b"i\0\0p\0\0q\0\0c\0\0");
    out
}

#[test]
fn claim_snapshot_capture_restore_install_preserves_receipts_and_indexes() -> TestResult {
    let source = f::directory("claim-snapshot-source")?;
    let (log, mut state, wire) = seed(&source)?;
    inspect(&mut state)?;
    assert_eq!(fixtures::artifact(&mut state)?, wire);
    drop((log, state));
    let expected = fixtures::cohort(&source)?;
    assert_eq!(expected[1].get(&due_key()), Some(&vec![0]));
    assert_eq!(expected[3], expected[1]);
    let restored = f::directory("claim-snapshot-restored")?;
    let candidate = stage(&restored.join("state.redb"), &wire)?;
    let mut state = candidate.into_store()?;
    inspect(&mut state)?;
    assert_eq!(fixtures::artifact(&mut state)?, wire);
    drop(state);
    assert_eq!(fixtures::cohort(&restored)?, expected);
    let target = f::directory("claim-snapshot-installed")?;
    let (log, mut state) = f::create_pair(&target)?;
    let candidate = stage(&target.join("candidate.redb"), &wire)?;
    let metadata = state.install_snapshot(candidate, SnapshotLimits::default())?;
    assert_eq!(metadata.id().as_slice(), Sha256::digest(&wire).as_slice());
    inspect(&mut state)?;
    assert_eq!(fixtures::artifact(&mut state)?, wire);
    drop((log, state));
    assert_eq!(fixtures::cohort(&target)?, expected);
    drop(f::open_pair(&target)?);
    assert_eq!(fixtures::cohort(&target)?, expected);
    for dir in [source, restored, target] {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn suffix(dir: &Path, complete: bool) -> TestResult<Vec<u8>> {
    let (mut log, mut state, wire) = seed(dir)?;
    let body = phase(12, b"changed")?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 3, &body)?;
    f::append(&mut log, 4, blank.as_bytes())?;
    if complete {
        apply(&mut state, 3, &body)?;
        f::apply_control(&mut state, 4, blank.as_bytes())?;
    }
    f::append(&mut log, 5, b"uncommitted opaque Claim suffix")?;
    log.save_committed(f::at(4))?;
    let s = f::scope()?;
    let cap = caps()?.with_claim_work_limits(ClaimWorkLimits::default());
    let (mut log, mut state) = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        cap,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(2)));
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(&wire).into())
    );
    assert_eq!(fixtures::artifact(&mut state)?, wire);
    Ok(wire)
}

#[test]
fn claim_purged_startup_audits_and_replays_original_leafs() -> TestResult {
    let reference = f::directory("claim-purged-reference")?;
    let wire = suffix(&reference, true)?;
    let expected = fixtures::cohort(&reference)?;
    let logs = f::log_rows(&reference)?;
    let log_meta = f::log_meta(&reference)?;
    let dir = f::directory("claim-purged-replay")?;
    assert_eq!(suffix(&dir, false)?, wire);
    let s = f::scope()?;
    for _ in 0..2 {
        let (log, state) = f::open_pair(&dir)?;
        let (mut log, mut state) = recover_snapshotted(
            log,
            state,
            s.group(),
            s.recovery(),
            caps()?.with_claim_work_limits(ClaimWorkLimits::default()),
            SnapshotLimits::default(),
        )?;
        inspect(&mut state)?;
        assert_eq!(log.state()?.last_purged(), Some(f::at(2)));
        assert_eq!(log.state()?.committed(), Some(f::at(4)));
        assert_eq!(log.state()?.last_log(), Some(f::at(5)));
        assert_eq!(fixtures::artifact(&mut state)?, wire);
        drop((log, state));
        assert_eq!(fixtures::cohort(&dir)?, expected);
        assert_eq!(f::log_rows(&dir)?, logs);
        assert_eq!(f::log_meta(&dir)?, log_meta);
    }
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

fn frame(meta: &f::Rows, rows: &f::Rows) -> TestResult<Vec<u8>> {
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

fn mutate_response(rows: &mut f::Rows, time: bool) -> TestResult {
    let value = rows
        .iter_mut()
        .find(|(key, value)| key.get(6) == Some(&2) && value.windows(4).any(|part| part == b"FWCR"))
        .ok_or("Claim response row absent")?
        .1;
    if time {
        let mut offset = 6;
        for _ in 0..4 {
            let length =
                usize::try_from(u32::from_be_bytes(value[offset..offset + 4].try_into()?))?;
            offset += 4 + length;
        }
        offset += 34 + 24; // Original intent plus term/node/index precede original time.
        value[offset..offset + 8].copy_from_slice(&12_u64.to_be_bytes());
    } else {
        let token = value
            .windows(6)
            .position(|part| part == b"w:11:1")
            .ok_or("token fixture absent")?;
        value[token + 5] = b'2';
    }
    Ok(())
}

#[test]
fn claim_snapshot_rejects_mutated_response_or_index_even_with_rehashed_envelope() -> TestResult {
    let source = f::directory("claim-snapshot-corruption")?;
    drop(seed(&source)?);
    let original = fixtures::cohort(&source)?;
    for (index, mode) in ["token", "time", "missing-due", "wrong-due"]
        .into_iter()
        .enumerate()
    {
        let mut rows = original[3].clone();
        match mode {
            "token" => mutate_response(&mut rows, false)?,
            "time" => mutate_response(&mut rows, true)?,
            "missing-due" => {
                assert!(rows.remove(&due_key()).is_some());
            }
            "wrong-due" => {
                assert_eq!(rows.insert(due_key(), vec![1]), Some(vec![0]));
            }
            _ => return Err("unknown corruption mode".into()),
        }
        let wire = frame(&original[2], &rows)?;
        assert_eq!(
            &wire[wire.len() - 32..],
            Sha256::digest(&wire[..wire.len() - 32]).as_slice()
        );
        let error = stage(&source.join(format!("bad-{index}.redb")), &wire)
            .err()
            .ok_or("semantic Claim corruption accepted after correct checksum")?;
        assert_eq!(
            error.downcast_ref::<SnapshotError>(),
            Some(&SnapshotError::Stage(StateTransactionError::Corrupt))
        );
        assert_eq!(fixtures::cohort(&source)?, original);
    }
    let mut valid = stage(
        &source.join("valid.redb"),
        &frame(&original[2], &original[3])?,
    )?
    .into_store()?;
    inspect(&mut valid)?;
    drop(valid);
    assert_eq!(fixtures::cohort(&source)?, original);
    std::fs::remove_dir_all(source)?;
    Ok(())
}
