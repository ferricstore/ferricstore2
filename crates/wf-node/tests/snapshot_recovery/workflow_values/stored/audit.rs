//! Explicit opt-in, actual persisted capability and complete historical audit.

use sha2::{Digest, Sha256};
use wf_domain::workflow::control::EntryStamp;
use wf_ports::log::{CodecError, normal_recovery_header};
use wf_storage::transaction::WorkflowValueTransactionError;

use super::*;

#[test]
fn stored_recovery_checks_opt_in_and_actual_capability() -> TestResult {
    let dir = f::directory("stored-node-opt-in")?;
    let (frame, _) = pending(&dir, 5, Some(b"FSV2\x02opaque"), false)?;
    let result = recover(&dir, &limits()?);
    assert!(
        result.is_ok(),
        "healthy stored Node replay: {:?}",
        result.as_ref().err()
    );
    drop(result?);
    assert!(matches!(
        normal_recovery_header(frame.as_bytes(), v::limits()?.normal_limits().codec),
        Err(CodecError::UnsupportedVersion)
    ));
    refuse(
        &dir,
        &v::limits()?,
        Error::Codec(CodecError::UnsupportedVersion),
    )?;
    let binary_only =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
    refuse(
        &dir,
        &binary_only,
        Error::Codec(CodecError::UnsupportedVersion),
    )?;
    let old = f::directory("stored-node-profile1")?;
    let (mut log, mut state) = v::pair(&old)?;
    let policy = v::install(&mut log, &mut state, true, None, 0, 10)?;
    let frame = encode(5, b"entry", 100, vec![item(5, b"i", 100, None)?], &policy)?;
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    refuse(
        &old,
        &limits()?,
        Error::Value(WorkflowValueTransactionError::UnsupportedProfile),
    )?;
    std::fs::remove_dir_all(old)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn audit_input(dir: &Path, case: &str) -> TestResult {
    let (mut log, mut state, policy) = seed(dir)?;
    let frame = encode(5, b"entry", 100, vec![item(5, b"i", 100, None)?], &policy)?;
    let mut bytes = frame.as_bytes().to_vec();
    // Literal t/g/z and request entry: intent9 is31..33, digest33..65.
    assert_eq!(&bytes[..7], b"FWCP\0\x01\x01");
    assert_eq!(&bytes[31..33], 9_u16.to_be_bytes());
    match case {
        "footer" => {
            let last = bytes.last_mut().ok_or("footer")?;
            *last ^= 1;
        }
        "intent" => {
            bytes[33] ^= 1;
            let at = bytes.len() - 32;
            let digest = Sha256::digest(&bytes[..at]);
            bytes[at..].copy_from_slice(&digest);
        }
        "eof" => bytes.push(0),
        "healthy" => {}
        _ => return Err("audit case".into()),
    }
    f::append(&mut log, 1, &bytes)?;
    apply(&mut state, &frame, 1)?;
    let blank = f::blank(EntryStamp::Stamped(f::clock(200, 0, 200)?))?;
    f::append(&mut log, 2, blank.as_bytes())?;
    f::apply_control(&mut state, 2, blank.as_bytes())?;
    log.save_committed(f::at(2))?;
    drop((log, state));
    Ok(())
}

#[test]
fn stored_recovery_audits_historical_frames_and_zero_value_entries() -> TestResult {
    let healthy = f::directory("stored-node-audit-healthy")?;
    audit_input(&healthy, "healthy")?;
    let before = v::cohort(&healthy)?;
    let result = recover(&healthy, &limits()?);
    assert!(
        result.is_ok(),
        "healthy stored Node audit: {:?}",
        result.as_ref().err()
    );
    let (log, mut state) = result?;
    assert_eq!(v::control(&mut state)?.position(), f::at(2).index());
    assert_eq!(v::outcome(&mut state, b"entry")?.intent().version(), 9);
    drop((log, state));
    assert_eq!(v::cohort(&healthy)?, before);
    assert_eq!(
        before[1]
            .keys()
            .filter(|key| matches!(key.get(6), Some(8 | 9)))
            .count(),
        0
    );
    for (case, error) in [
        ("footer", CodecError::Malformed),
        ("intent", CodecError::Malformed),
        ("eof", CodecError::TrailingBytes),
    ] {
        let dir = f::directory("stored-node-audit-negative")?;
        audit_input(&dir, case)?;
        refuse(&dir, &limits()?, Error::Codec(error))?;
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(healthy)?;
    Ok(())
}
