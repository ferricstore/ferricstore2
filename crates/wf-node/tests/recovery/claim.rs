//! Real Claim dispatch, historical admission and shared startup resource limits.

use super::{CommittedRecoveryError as Error, CommittedRecoveryLimits, TestResult};
use super::{fixtures as f, limits, recover_committed};
use std::path::Path;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryStamp;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_domain::workflow::{Create, Operation};
use wf_ports::log::{CodecError, CodecLimits, RaftLogError, RaftLogStore, RecordBatchApplyError};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionError, ClaimTransactionLimits, ClaimTransactionOutcome,
    RecordBatchTransactionLimits, apply_claim_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};

fn raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

fn phase(
    s: &RequestScope,
    request: &[u8],
    now: u64,
    kind: &[u8],
    digest: u8,
) -> TestResult<Vec<u8>> {
    let mut out = b"FWCP\0\x01\x03".to_vec();
    for value in [
        s.tenant().as_bytes(),
        s.group().as_bytes(),
        s.recovery().as_bytes(),
        request,
    ] {
        raw(&mut out, value)?;
    }
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&[digest; 32]);
    out.extend_from_slice(&now.to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.extend_from_slice(&now.to_be_bytes());
    raw(&mut out, kind)?;
    out.extend_from_slice(&[0; 7]); // ANY states, no exclusions/partitions/priority.
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
    for field in [b"p".as_slice(), b"i", b"c", b"q", b"w", b"w:11:1"] {
        raw(&mut out, field)?;
    }
    for scalar in [2_u64, 1, 61, 10, 11, 0] {
        out.extend_from_slice(&scalar.to_be_bytes());
    }
    out.push(0);
    out.extend_from_slice(&(1_u128 << 32).to_be_bytes());
    out.push(1);
    raw(&mut out, &[0, 255])?;
    out.extend_from_slice(&[0, 0]);
    Ok(out)
}

fn empty_receipt() -> Vec<u8> {
    let mut out = b"FWCR\0\x01\0\0\0".to_vec();
    out.extend_from_slice(&[0; 12]);
    out
}

fn create() -> Operation {
    Operation::Create(Create {
        workflow_type: b"c".to_vec(),
        state: b"q".to_vec(),
        run_at: Some(wf_types::version::TimestampMillis::new(10)),
        payload: Some(vec![0, 255]),
    })
}

fn caps() -> TestResult<CommittedRecoveryLimits> {
    Ok(limits()?.with_claim_work_limits(ClaimWorkLimits::default()))
}

fn frames() -> TestResult<Vec<(u16, Vec<u8>)>> {
    let s = f::scope()?;
    Ok(vec![
        (0, f::member(EntryStamp::Unstamped, 4)?.as_bytes().to_vec()),
        (
            5,
            f::frame(5, b"create", 10, vec![(b"i", create())])?
                .as_bytes()
                .to_vec(),
        ),
        (0, f::blank(EntryStamp::Unstamped)?.as_bytes().to_vec()),
        (3, phase(&s, b"claim", 11, b"c", 7)?),
        (3, phase(&s, b"claim", 12, b"changed", 7)?),
        (3, phase(&s, b"claim", 12, b"changed", 8)?),
        (
            6,
            f::frame(6, b"independent", 13, vec![(b"j", create())])?
                .as_bytes()
                .to_vec(),
        ),
        (
            1,
            f::frame(1, b"public", 14, vec![(b"j", f::cancel_op())])?
                .as_bytes()
                .to_vec(),
        ),
        (3, phase(&s, b"claim", 1012, b"c", 7)?),
        (3, phase(&s, b"empty", 1013, b"absent", 9)?),
    ])
}

fn apply(state: &mut RedbStateStore, index: u64, profile: u16, body: &[u8]) -> TestResult {
    if profile == 0 {
        f::apply_control(state, index, body)?;
    } else if profile == 3 {
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
        match index {
            3 | 4 => {
                let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
                    return Err("original Claim reply missing".into());
                };
                assert_eq!(outcome.response(), receipt()?);
                assert_eq!(outcome.response_version(), 3);
                assert_eq!(outcome.original_position(), f::at(3));
                assert_eq!(outcome.original_time().get(), 11);
                assert_eq!(outcome.expires_at().get(), 1011);
                assert_eq!(outcome.status(), RetainedStatus::Accepted);
            }
            5 => assert!(matches!(result, ClaimTransactionOutcome::IntentConflict(_))),
            8 => assert!(matches!(result, ClaimTransactionOutcome::Expired(_))),
            9 => {
                let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
                    return Err("empty Claim reply missing".into());
                };
                assert_eq!(outcome.response(), empty_receipt());
                assert_eq!(outcome.original_time().get(), 1013);
            }
            _ => return Err("unclassified Claim fixture".into()),
        }
    } else {
        let result = f::apply_normal(state, index, body, profile)?;
        let wf_storage::transaction::RecordBatchTransactionOutcome::Retained { outcome, .. } =
            result
        else {
            return Err("mixed-profile setup failed".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(
            outcome.response(),
            if profile == 6 {
                &[2, 0, 0, 0, 1, 0][..]
            } else {
                &[0][..]
            }
        );
    }
    Ok(())
}

fn build(dir: &Path, prefix: usize) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    for (index, (profile, body)) in frames()?.iter().enumerate() {
        let index_u64 = u64::try_from(index)?;
        f::append(&mut log, index_u64, body)?;
        if index < prefix {
            apply(&mut state, index_u64, *profile, body)?;
        }
    }
    f::append(&mut log, 10, b"opaque uncommitted Claim tail")?;
    log.save_committed(f::at(9))?;
    Ok(())
}

fn refusal(
    dir: &Path,
    s: &RequestScope,
    cap: CommittedRecoveryLimits,
    expected: Error,
) -> TestResult {
    let before = (
        f::state_rows(dir)?,
        f::state_meta(dir)?,
        f::log_rows(dir)?,
        f::log_meta(dir)?,
    );
    let (log, state) = f::open_pair_for(dir, s)?;
    let error = recover_committed(log, state, s.group(), s.recovery(), cap)
        .err()
        .ok_or("unpaid or malformed Claim startup accepted")?;
    assert_eq!(error, expected);
    drop(f::open_pair_for(dir, s)?);
    assert_eq!(
        (
            f::state_rows(dir)?,
            f::state_meta(dir)?,
            f::log_rows(dir)?,
            f::log_meta(dir)?
        ),
        before
    );
    Ok(())
}

fn inspect_original(state: &mut RedbStateStore) -> TestResult {
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        16 * f::MIB,
        16 * f::MIB,
    )?;
    let outcome = tx
        .outcome(s.tenant(), &RequestId::new(b"claim".to_vec())?)?
        .ok_or("persisted original Claim missing")?;
    assert_eq!(outcome.response(), receipt()?);
    assert_eq!(outcome.original_position(), f::at(3));
    assert_eq!(outcome.original_time().get(), 11);
    assert_eq!(outcome.expires_at().get(), 1011);
    let row = tx
        .record(
            s.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("Claim core missing")?;
    assert_eq!(
        (
            row.record.state(),
            row.record.version(),
            row.record.fence().get()
        ),
        (b"running".as_slice(), 2, 1)
    );
    assert_eq!(row.ordering.state_enter_seq, 1_u128 << 32);
    assert_eq!(
        row.record
            .lease()
            .ok_or("Claim lease missing")?
            .token
            .as_bytes(),
        b"w:11:1"
    );
    Ok(())
}

#[test]
fn mixed_committed_claim_replay_preserves_original_outcomes() -> TestResult {
    let reference = f::directory("claim-mixed-reference")?;
    build(&reference, 10)?;
    let rows = f::state_rows(&reference)?;
    let meta = f::state_meta(&reference)?;
    let s = f::scope()?;
    for prefix in [0, 2, 4, 6, 10] {
        let dir = f::directory("claim-mixed-replay")?;
        build(&dir, prefix)?;
        let logs = f::log_rows(&dir)?;
        let log_meta = f::log_meta(&dir)?;
        for _ in 0..2 {
            let (log, state) = f::open_pair(&dir)?;
            let (mut log, mut state) =
                recover_committed(log, state, s.group(), s.recovery(), caps()?)?;
            assert_eq!(log.state()?.committed(), Some(f::at(9)));
            assert_eq!(log.state()?.last_log(), Some(f::at(10)));
            inspect_original(&mut state)?;
            assert_eq!(
                f::applied(&mut state)?.1.entry().map(|entry| entry.log()),
                Some(f::at(0))
            );
            drop((log, state));
            assert_eq!(f::state_rows(&dir)?, rows);
            assert_eq!(f::state_meta(&dir)?, meta);
            assert_eq!(f::log_rows(&dir)?, logs);
            assert_eq!(f::log_meta(&dir)?, log_meta);
        }
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

fn audited(dir: &Path, malformed: bool) -> TestResult<usize> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let s = f::scope()?;
    let body = phase(&s, b"audit", 1, b"absent", 1)?;
    let length = body.len();
    let stored = if malformed {
        &body[..length - 1]
    } else {
        &body
    };
    f::append(&mut log, 0, stored)?;
    let result = apply_claim_transaction(
        &mut state,
        &s,
        &body,
        f::at(0),
        ClaimTransactionLimits {
            base: f::driver_limits(),
            work: ClaimWorkLimits::default(),
        },
    )?;
    let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("audit setup failed".into());
    };
    assert_eq!(outcome.response(), empty_receipt());
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 1, blank.as_bytes())?;
    f::apply_control(&mut state, 1, blank.as_bytes())?;
    let suffix = f::frame(5, b"must-not-run", 2, vec![(b"i", create())])?;
    f::append(&mut log, 2, suffix.as_bytes())?;
    log.save_committed(f::at(2))?;
    Ok(length)
}

#[test]
fn claim_audit_validates_full_op3_body_and_profiles_before_any_replay() -> TestResult {
    let s = f::scope()?;
    for case in ["malformed", "no-profile", "wire-profile"] {
        let dir = f::directory(case)?;
        let length = audited(&dir, case == "malformed")?;
        let (cap, error) = match case {
            "malformed" => (
                caps()?,
                Error::Claim(ClaimTransactionError::Apply(
                    RecordBatchApplyError::Encoding(CodecError::Malformed),
                )),
            ),
            "no-profile" => (limits()?, Error::UnsupportedEntry),
            _ => {
                let mut normal = f::driver_limits();
                normal.codec = CodecLimits::new(AdmissionLimits::new(1024, length - 1, 1000)?);
                (
                    profile(normal, (1000, 64 * f::MIB), 16 * f::MIB, 512 * f::MIB)?,
                    Error::Codec(CodecError::ByteLimit),
                )
            }
        };
        refusal(&dir, &s, cap, error)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn claim_replay_failure_retains_only_completed_prefix() -> TestResult {
    let reference = f::directory("claim-prefix-reference")?;
    build(&reference, 3)?;
    let prefix = (f::state_rows(&reference)?, f::state_meta(&reference)?);
    let (log, mut state) = f::open_pair(&reference)?;
    for (index, (kind, body)) in frames()?.iter().enumerate().skip(3) {
        apply(&mut state, u64::try_from(index)?, *kind, body)?;
    }
    drop((log, state));
    let complete = (f::state_rows(&reference)?, f::state_meta(&reference)?);
    let dir = f::directory("claim-prefix-failure")?;
    build(&dir, 2)?;
    let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
    let mut normal = f::driver_limits();
    normal.workspace_bytes = 1;
    let cap = CommittedRecoveryLimits::new(
        wf_ports::consensus::RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal,
        (1000, 64 * f::MIB),
        16 * f::MIB,
        512 * f::MIB,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default());
    let (log, state) = f::open_pair(&dir)?;
    let s = f::scope()?;
    let error = recover_committed(log, state, s.group(), s.recovery(), cap)
        .err()
        .ok_or("tiny Claim driver workspace accepted")?;
    assert!(matches!(
        error,
        Error::Claim(ClaimTransactionError::Apply(
            RecordBatchApplyError::Workspace { maximum: 1, .. }
        ))
    ));
    drop(f::open_pair(&dir)?);
    assert_eq!((f::state_rows(&dir)?, f::state_meta(&dir)?), prefix);
    assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
    )?);
    assert_eq!((f::state_rows(&dir)?, f::state_meta(&dir)?), complete);
    assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

fn profile(
    normal: RecordBatchTransactionLimits,
    work: (usize, usize),
    entry: usize,
    workspace: usize,
) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        wf_ports::consensus::RaftMembershipLimits::default(),
        (100, 1536, 1024, 131),
        normal,
        work,
        entry,
        workspace,
    )?
    .with_claim_work_limits(ClaimWorkLimits::default()))
}

fn scope_peak() -> TestResult {
    let dir = f::directory("claim-large-scope")?;
    let s = f::scope_for(&[255; 4096], b"g", b"z")?;
    let (mut log, state) = f::create_pair_for(&dir, &s)?;
    let body = phase(&s, b"empty", 1, b"absent", 1)?;
    f::append(&mut log, 0, &body)?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let normal = f::driver_limits();
    let q = 512 + 4096 + 1 + 1;
    let exact = 3588 + 512 + q + normal.workspace_bytes;
    refusal(
        &dir,
        &s,
        profile(normal, (1, body.len()), 512 + body.len(), exact - 1)?,
        Error::Limit,
    )?;
    let (log, state) = f::open_pair_for(&dir, &s)?;
    let (log, mut state) = recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(normal, (1, body.len()), 512 + body.len(), exact)?,
    )?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        16 * f::MIB,
        16 * f::MIB,
    )?;
    let outcome = tx
        .outcome(s.tenant(), &RequestId::new(b"empty".to_vec())?)?
        .ok_or("large-scope outcome missing")?;
    assert_eq!(outcome.response(), empty_receipt());
    assert_eq!(outcome.scope().tenant(), s.tenant());
    drop(tx);
    drop((log, state));
    drop(f::open_pair_for(&dir, &s)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn claim_startup_exact_tight_point_body_scope_and_driver_peak() -> TestResult {
    let dir = f::directory("claim-audit-budget")?;
    let s = f::scope()?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    let body = phase(&s, b"empty", 1, b"absent", 1)?;
    f::append(&mut log, 0, &body)?;
    drop(apply_claim_transaction(
        &mut state,
        &s,
        &body,
        f::at(0),
        ClaimTransactionLimits {
            base: f::driver_limits(),
            work: ClaimWorkLimits::default(),
        },
    )?);
    log.save_committed(f::at(0))?;
    drop((log, state));
    let bytes = body.len();
    let d = 4 * bytes + 2048;
    let exact = 3588 + 512 + d;
    refusal(
        &dir,
        &s,
        profile(f::driver_limits(), (1, bytes), 512 + bytes, exact - 1)?,
        Error::Claim(ClaimTransactionError::Apply(
            RecordBatchApplyError::Workspace {
                attempted: d,
                maximum: d - 1,
            },
        )),
    )?;
    for (wire, entry) in [(bytes - 1, 512 + bytes), (bytes, 511 + bytes)] {
        refusal(
            &dir,
            &s,
            profile(f::driver_limits(), (1, wire), entry, exact)?,
            Error::Log(RaftLogError::Limit),
        )?;
    }
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(f::driver_limits(), (1, bytes), 512 + bytes, exact)?,
    )?);
    let (mut log, state) = f::open_pair(&dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    assert_eq!(blank.as_bytes().len(), 13);
    f::append(&mut log, 1, blank.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    refusal(
        &dir,
        &s,
        profile(f::driver_limits(), (1, bytes + 13), 512 + bytes, exact)?,
        Error::Limit,
    )?;
    refusal(
        &dir,
        &s,
        profile(f::driver_limits(), (2, bytes + 12), 512 + bytes, exact)?,
        Error::Log(RaftLogError::Limit),
    )?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(f::driver_limits(), (2, bytes + 13), 512 + bytes, exact)?,
    )?);
    std::fs::remove_dir_all(dir)?;
    scope_peak()
}
