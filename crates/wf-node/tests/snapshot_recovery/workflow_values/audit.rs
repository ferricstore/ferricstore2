//! An earlier malformed eight body cannot hide behind a valid last Blank.

use sha2::{Digest, Sha256};
use wf_domain::workflow::control::EntryStamp;
use wf_ports::consensus::WorkflowValueTargetGuard;
use wf_ports::log::{CodecError, RaftLogStore};

use super::{Error, TestResult, f, fixtures as v, recover_committed};

#[test]
fn value_recovery_audits_complete_applied_frames() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    for case in [
        "healthy",
        "footer",
        "intent",
        "eof",
        "metadata",
        "last-body",
        "foreign",
        "stamp",
        "server-context",
    ] {
        audit_case(case, &limits)?;
    }
    Ok(())
}

fn audit_case(case: &str, limits: &super::Limits) -> TestResult {
    let dir = f::directory("value-complete-audit")?;
    let (mut log, mut state) = v::pair(&dir)?;
    let policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    let mut batch = v::batch(
        &f::scope()?,
        5,
        b"audit",
        100,
        vec![v::create(b"i", 100, b"A")?],
        policy,
    )?;
    let valid = v::encode(&batch)?;
    let mut bytes = valid.as_bytes().to_vec();
    if case == "server-context" {
        batch.defaults = v::defaults(9_000);
        batch.policy_references_captured = None;
        batch.policies[0].generation = 99;
        batch.policies[0].digest = wf_domain::workflow::control::PreparedDigest::new([0xa5; 32]);
        batch.inputs[0].target = WorkflowValueTargetGuard::Present { incarnation: 999 };
        bytes = v::encode(&batch)?.into_bytes();
    } else {
        mutate(&mut bytes, case)?;
    }
    f::append(&mut log, 1, &bytes)?;
    // The actual stored record is produced only by the valid original driver.
    v::accepted(v::apply(&mut state, &valid, 1)?, 1, 100, 5)?;
    let blank = f::blank(EntryStamp::Stamped(f::clock(200, 0, 200)?))?;
    let logged = if case == "last-body" {
        f::blank(EntryStamp::Stamped(f::clock(200, 1, 200)?))?
    } else {
        f::blank(EntryStamp::Stamped(f::clock(200, 0, 200)?))?
    };
    f::append(&mut log, 2, logged.as_bytes())?;
    f::apply_control(&mut state, 2, blank.as_bytes())?;
    log.save_committed(f::at(2))?;
    drop((log, state, batch, valid, blank, logged));
    let expected = match case {
        "footer" | "intent" | "stamp" => Some(Error::Codec(CodecError::Malformed)),
        "eof" => Some(Error::Codec(CodecError::TrailingBytes)),
        "metadata" => Some(Error::Codec(CodecError::InvalidOptionality)),
        "last-body" => Some(Error::BodyMismatch),
        "foreign" => Some(Error::NamespaceMismatch),
        "healthy" | "server-context" => None,
        _ => return Err("unknown audit expectation".into()),
    };
    if let Some(expected) = expected {
        v::refused(&dir, limits, expected)?;
    } else {
        let before = v::cohort(&dir)?;
        let scope = f::scope()?;
        let (log, state) = f::open_pair(&dir)?;
        let (log, mut state) =
            recover_committed(log, state, scope.group(), scope.recovery(), *limits)?;
        assert_eq!(
            v::control(&mut state)?
                .last_entry()
                .map(wf_domain::workflow::control::EntryIdentity::log),
            Some(f::at(2))
        );
        drop((log, state));
        assert_eq!(v::cohort(&dir)?, before);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn mutate(bytes: &mut Vec<u8>, case: &str) -> TestResult {
    // Scope t/g/z, request "audit": intent digest begins33, stamp65, body83.
    assert_eq!(&bytes[..7], b"FWCP\0\x01\x01");
    assert_eq!(&bytes[31..33], &8_u16.to_be_bytes());
    match case {
        "footer" => {
            let last = bytes.last_mut().ok_or("missing footer")?;
            *last ^= 1;
        }
        "intent" => bytes[33] ^= 0x40,
        "eof" => bytes.push(0),
        "metadata" => bytes[85] = 2,
        "foreign" => bytes[16] = b'x',
        "stamp" => bytes[65..73].copy_from_slice(&u64::MAX.to_be_bytes()),
        "healthy" | "last-body" => {}
        _ => return Err("unknown audit mutation".into()),
    }
    if matches!(case, "intent" | "metadata" | "foreign" | "stamp") {
        let at = bytes.len().checked_sub(32).ok_or("short footer")?;
        let digest = Sha256::digest(&bytes[..at]);
        bytes[at..].copy_from_slice(&digest);
    }
    Ok(())
}
