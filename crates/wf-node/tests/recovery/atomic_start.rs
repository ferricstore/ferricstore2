//! Actual atomic drivers, complete historical audits and consumed-prefix custody.

use super::{
    CommittedRecoveryError as Error, CommittedRecoveryLimits, TestResult, assert_refusal,
    fixtures as f, limits, recover_committed,
};
use std::path::Path;
use wf_domain::history::EventKind;
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_ports::log::RecordBatchApplyError::{Encoding, Workspace};
use wf_ports::log::{CodecError, CodecLimits, PreparedBytes, RaftLogError, RaftLogStore};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::state::{key, value};
use wf_storage::transaction::RecordBatchTransactionError::Apply;
use wf_storage::transaction::RecordBatchTransactionLimits;
use wf_types::identity::{PartitionId, RecordId};

fn history(dir: &Path) -> TestResult<Vec<(u16, PreparedBytes)>> {
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
    let mut frames = vec![
        (5, f::frame(5, b"b", 1, vec![(b"b", f::create_op())])?),
        (
            7,
            f::frame(7, b"q", 17, vec![(b"i", f::atomic_start_op(b"u"))])?,
        ),
    ];
    for (index, (profile, frame)) in frames.iter().enumerate() {
        let index = u64::try_from(index)?;
        f::append(&mut log, index, frame.as_bytes())?;
        drop(f::apply_normal(
            &mut state,
            index,
            frame.as_bytes(),
            *profile,
        )?);
    }
    frames.push((5, f::atomic_complete_frame(&mut state)?));
    frames.push((
        7,
        f::frame(7, b"q", 18, vec![(b"i", f::atomic_start_op(b"u"))])?,
    ));
    frames.push((0, f::blank(EntryStamp::Unstamped)?));
    for (index, (profile, frame)) in frames.iter().enumerate().skip(2) {
        let index = u64::try_from(index)?;
        f::append(&mut log, index, frame.as_bytes())?;
        if *profile == 0 {
            f::apply_control(&mut state, index, frame.as_bytes())?;
        } else {
            drop(f::apply_normal(
                &mut state,
                index,
                frame.as_bytes(),
                *profile,
            )?);
        }
    }
    drop(f::atomic_original(&mut state)?);
    f::append(&mut log, 5, b"opaque uncommitted atomic tail")?;
    log.save_committed(f::at(4))?;
    Ok(frames)
}

fn seed(
    dir: &Path,
    frames: &[(u16, PreparedBytes)],
    prefix: usize,
    corrupt: Option<&[u8]>,
) -> TestResult {
    let (mut log, mut state) = f::create_pair(dir)?;
    for (index, (profile, frame)) in frames.iter().enumerate() {
        let at = u64::try_from(index)?;
        let body = if index == 1 {
            corrupt.unwrap_or(frame.as_bytes())
        } else {
            frame.as_bytes()
        };
        f::append(&mut log, at, body)?;
        if index < prefix {
            if *profile == 0 {
                f::apply_control(&mut state, at, frame.as_bytes())?;
            } else {
                drop(f::apply_normal(&mut state, at, frame.as_bytes(), *profile)?);
            }
        }
    }
    let count = u64::try_from(frames.len())?;
    f::append(&mut log, count, b"opaque uncommitted atomic tail")?;
    log.save_committed(f::at(count - 1))?;
    Ok(())
}

fn profile(
    normal: RecordBatchTransactionLimits,
    work: (usize, usize),
    point: usize,
    workspace: usize,
) -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        wf_ports::consensus::RaftMembershipLimits::default(),
        (100, 1536, 1024, 131),
        normal,
        work,
        point,
        workspace,
    )?)
}

#[test]
fn atomic_start_committed_recovery_preserves_original_after_completion() -> TestResult {
    let reference = f::directory("atomic-recovery-reference")?;
    let frames = history(&reference)?;
    let rows = f::state_rows(&reference)?;
    let meta = f::state_meta(&reference)?;
    let s = f::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let prefix = key::encode_history_prefix(&s, &partition, &id, f::MIB)?;
    let events = rows
        .iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .map(|(_, bytes)| value::decode_history(bytes, &s, &partition, &id, f::MIB))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(events.len(), 2);
    assert_eq!(
        (events[0].kind, events[0].event_id.as_str()),
        (EventKind::Created, "17-1")
    );
    assert_eq!(
        (events[1].kind, events[1].event_id.as_str()),
        (EventKind::Completed, "17-2")
    );
    for applied in [0, 1, 2, 3, 5] {
        let dir = f::directory("atomic-recovery-prefix")?;
        seed(&dir, &frames, applied, None)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        for _ in 0..2 {
            let (log, state) = f::open_pair(&dir)?;
            let (mut log, mut state) =
                recover_committed(log, state, s.group(), s.recovery(), limits()?)?;
            assert_eq!(log.state()?.committed(), Some(f::at(4)));
            assert_eq!(log.state()?.last_log(), Some(f::at(5)));
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
            let current = tx
                .record(s.tenant(), &partition, &id)?
                .ok_or("current absent")?;
            assert_eq!(current.record.state(), b"completed");
            assert_eq!(current.record.version(), 2);
            assert_eq!(current.record.result(), Some(b"done".as_slice()));
            assert!(current.record.lease().is_none());
            drop(tx);
            assert_eq!(
                f::applied(&mut state)?
                    .0
                    .last_entry()
                    .map(EntryIdentity::log),
                Some(f::at(4))
            );
            drop((log, state));
            assert_eq!(f::state_rows(&dir)?, rows);
            assert_eq!(f::state_meta(&dir)?, meta);
            assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        }
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

#[test]
fn atomic_start_recovery_audits_complete_body_before_suffix_publication() -> TestResult {
    let reference = f::directory("atomic-audit-reference")?;
    let mut frames = history(&reference)?;
    frames.push((
        5,
        f::frame(5, b"later", 19, vec![(b"later", f::create_op())])?,
    ));
    for mode in [
        "boundary",
        "trailing",
        "intent",
        "operation",
        "namespace",
        "header-cap",
    ] {
        let dir = f::directory(mode)?;
        let mut changed = frames[1].1.as_bytes().to_vec();
        let mut cap = limits()?;
        let expected = match mode {
            "boundary" => {
                changed.push(0);
                Error::BodyMismatch
            }
            "namespace" => {
                changed[16] = b'X';
                changed.push(0);
                Error::NamespaceMismatch
            }
            "header-cap" => {
                let mut normal = f::driver_limits();
                normal.codec = CodecLimits::new(AdmissionLimits::new(1024, changed.len() - 1, 1)?);
                cap = profile(normal, (100, 64 * f::MIB), 16 * f::MIB, 512 * f::MIB)?;
                Error::Codec(CodecError::ByteLimit)
            }
            _ => {
                let error = match mode {
                    "trailing" => {
                        changed.push(0);
                        Encoding(CodecError::TrailingBytes)
                    }
                    "intent" => {
                        changed[29] ^= 0x40;
                        wf_ports::log::RecordBatchApplyError::IntentMismatch
                    }
                    _ => {
                        changed[102] = 12;
                        Encoding(CodecError::UnsupportedTag)
                    }
                };
                Error::Normal(Apply(error))
            }
        };
        seed(
            &dir,
            &frames,
            if mode == "boundary" { 2 } else { 5 },
            Some(&changed),
        )?;
        assert_refusal(&dir, cap, expected)?;
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(reference)?;
    Ok(())
}

#[test]
fn atomic_start_recovery_admits_exact_log_audit_and_driver_owners() -> TestResult {
    let reference = f::directory("atomic-budget-source")?;
    let mut frames = history(&reference)?;
    frames.truncate(2);
    let dir = f::directory("atomic-audit-budget")?;
    seed(&dir, &frames, 2, None)?;
    let s = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        limits()?,
    )?);
    let e = frames[1].1.as_bytes().len();
    let wire = frames
        .iter()
        .map(|(_, frame)| frame.as_bytes().len())
        .sum::<usize>();
    let mut normal = f::driver_limits();
    normal.codec = CodecLimits::new(AdmissionLimits::new(1024, e, 1)?);
    let d = 4 * e + e + 3072;
    let exact = 3588 + 512 + d;
    let header = std::mem::size_of::<wf_ports::log::DecodedRequest<'static>>();
    let funded = exact.checked_add(header).ok_or("header budget overflow")?;
    assert!(matches!(
        wf_ports::log::prepare_atomic_start(frames[1].1.as_bytes(), normal.codec, d - 1),
        Err(Workspace { attempted, maximum }) if attempted == d && maximum == d - 1
    ));
    let first = 4 * frames[0].1.as_bytes().len() + e + 3072;
    for (work, point, workspace, expected) in [
        ((2, wire), 511 + e, funded, Error::Log(RaftLogError::Limit)),
        (
            (2, wire - 1),
            512 + e,
            funded,
            Error::Log(RaftLogError::Limit),
        ),
        ((1, wire), 512 + e, funded, Error::Limit),
        (
            (2, wire),
            512 + e,
            exact - 1,
            Error::Normal(Apply(Workspace {
                attempted: first + header,
                maximum: d - 1,
            })),
        ),
        (
            (2, wire),
            512 + e,
            funded - 1,
            Error::Normal(Apply(Workspace {
                attempted: d + header,
                maximum: d + header - 1,
            })),
        ),
    ] {
        assert_refusal(&dir, profile(normal, work, point, workspace)?, expected)?;
    }
    let (log, state) = f::open_pair(&dir)?;
    let cap = profile(normal, (2, wire), 512 + e, funded)?;
    drop(recover_committed(log, state, s.group(), s.recovery(), cap)?);
    normal.codec = CodecLimits::new(AdmissionLimits::new(2, e, 1)?);
    assert_refusal(
        &dir,
        profile(normal, (2, wire), 512 + e, funded)?,
        Error::Normal(Apply(Encoding(CodecError::ValueLimit))),
    )?;
    std::fs::remove_dir_all(dir)?;
    check_atomic_driver_peaks(&mut frames, reference)
}

fn check_atomic_driver_peaks(
    frames: &mut Vec<(u16, PreparedBytes)>,
    reference: std::path::PathBuf,
) -> TestResult {
    let s = f::scope()?;
    let e = frames[1].1.as_bytes().len();
    let wire = frames
        .iter()
        .map(|(_, frame)| frame.as_bytes().len())
        .sum::<usize>();
    let outer = f::directory("atomic-outer-envelope")?;
    seed(&outer, frames, 1, None)?;
    let mut normal = f::driver_limits();
    let peak = 3588 + 512 + 515 + normal.workspace_bytes;
    assert_refusal(
        &outer,
        profile(normal, (2, wire), 512 + e, peak - 1)?,
        Error::Limit,
    )?;
    let (log, state) = f::open_pair(&outer)?;
    let cap = profile(normal, (2, wire), 512 + e, peak)?;
    drop(recover_committed(log, state, s.group(), s.recovery(), cap)?);
    std::fs::remove_dir_all(outer)?;
    frames.push((0, f::blank(EntryStamp::Unstamped)?));
    frames.push((
        7,
        f::frame(7, b"q", 18, vec![(b"i", f::atomic_start_op(b"u"))])?,
    ));
    let expected = f::directory("atomic-inner-prefix")?;
    seed(&expected, frames, 3, None)?;
    let dir = f::directory("atomic-inner-refusal")?;
    seed(&dir, frames, 2, None)?;
    let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
    normal.workspace_bytes = 1;
    let (log, state) = f::open_pair(&dir)?;
    let cap = profile(normal, (100, 64 * f::MIB), 16 * f::MIB, 512 * f::MIB)?;
    let error = recover_committed(log, state, s.group(), s.recovery(), cap)
        .err()
        .ok_or("tiny atomic driver accepted")?;
    assert_eq!(
        error,
        Error::Normal(Apply(Workspace {
            attempted: 160 * f::MIB,
            maximum: 1,
        }))
    );
    drop(f::open_pair(&dir)?);
    assert_eq!(f::state_rows(&dir)?, f::state_rows(&expected)?);
    assert_eq!(f::state_meta(&dir)?, f::state_meta(&expected)?);
    assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
    for dir in [dir, expected, reference] {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
