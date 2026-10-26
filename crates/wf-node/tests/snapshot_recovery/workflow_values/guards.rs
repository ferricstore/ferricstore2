//! Capability and live authority refusals keep the complete databases unchanged.

use sha2::{Digest, Sha256};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::{Operation, Transition};
use wf_ports::consensus::WorkflowValueTargetGuard;
use wf_ports::log::{CodecError, RaftLogStore};
use wf_ports::state::StateTransactionError;
use wf_storage::transaction::{RecordBatchTransactionError, WorkflowValueTransactionError};
use wf_types::version::Fence;

use super::{Error, TestResult, f, fixtures as v, recover_committed};

#[test]
fn value_recovery_opt_in_and_layout_guards_remain_fail_closed() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    disabled_then_enabled(&limits)?;
    populated_legacy(&limits)?;
    legacy_business(&limits)?;
    unknown_mode(&limits)?;
    for case in 0..9 {
        live_guard(case, &limits)?;
    }
    Ok(())
}

fn disabled_then_enabled(limits: &super::Limits) -> TestResult {
    let dir = f::directory("value-disabled")?;
    let (mut log, mut state) = v::pair(&dir)?;
    let policy = v::install(&mut log, &mut state, true, None, 0, 10)?;
    let frame = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"enabled",
        100,
        vec![v::create(b"i", 100, b"A")?],
        v::copy_policy(&policy),
    )?)?;
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    assert!(v::limits()?.workflow_value_limits().is_none());
    v::refused(&dir, &v::limits()?, Error::UnsupportedEntry)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    let (log, state) = recover_committed(log, state, scope.group(), scope.recovery(), *limits)?;
    v::binding(
        &state,
        b"i",
        None,
        (1_u128 << 32, [Some(1), None, None], None),
        &policy,
    )?;
    drop((log, state, frame));
    drop(f::open_pair(&dir)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn populated_legacy(limits: &super::Limits) -> TestResult {
    let dir = f::directory("value-legacy-layout")?;
    let (mut log, mut state) = f::create_pair(&dir)?;
    let old = f::frame(5, b"legacy", 10, vec![(b"old", f::create_op())])?;
    f::append(&mut log, 0, old.as_bytes())?;
    drop(f::apply_normal(&mut state, 0, old.as_bytes(), 5)?);
    let frame = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"new",
        20,
        vec![v::create(b"i", 20, b"A")?],
        v::witness(None)?,
    )?)?;
    f::append(&mut log, 1, frame.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    let before = super::super::fixtures::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let meta = f::log_meta(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), *limits).err(),
        Some(Error::Value(
            WorkflowValueTransactionError::UnsupportedProfile
        )),
    );
    drop(f::open_pair(&dir)?);
    assert_eq!(super::super::fixtures::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn legacy_business(limits: &super::Limits) -> TestResult {
    let dir = f::directory("value-old-business")?;
    let (mut log, mut state) = v::pair(&dir)?;
    v::install(&mut log, &mut state, false, None, 0, 10)?;
    let old = f::frame(5, b"bypass", 20, vec![(b"i", f::create_op())])?;
    f::append(&mut log, 1, old.as_bytes())?;
    log.save_committed(f::at(1))?;
    drop((log, state));
    v::refused(
        &dir,
        limits,
        Error::Normal(RecordBatchTransactionError::Transaction(
            StateTransactionError::UnsupportedLayout,
        )),
    )?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn unknown_mode(limits: &super::Limits) -> TestResult {
    let dir = f::directory("value-unknown-mode")?;
    let (mut log, mut state) = v::pair(&dir)?;
    let policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    let frame = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"guard",
        100,
        vec![v::create(b"i", 100, b"A")?],
        policy,
    )?)?;
    let mut bytes = frame.into_bytes();
    assert_eq!(bytes[84], 5);
    bytes[84] = u8::MAX;
    let at = bytes.len().checked_sub(32).ok_or("missing footer")?;
    let hash = Sha256::digest(&bytes[..at]);
    bytes[at..].copy_from_slice(&hash);
    f::append(&mut log, 1, &bytes)?;
    log.save_committed(f::at(1))?;
    drop((log, state, bytes));
    v::refused(
        &dir,
        limits,
        Error::Value(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Apply(wf_ports::log::RecordBatchApplyError::Encoding(
                CodecError::UnsupportedTag,
            )),
        )),
    )?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn live_guard(case: u8, limits: &super::Limits) -> TestResult {
    let dir = f::directory("value-live-guard")?;
    let (mut log, mut state) = v::pair(&dir)?;
    let mut policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    let birth = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"birth",
        20,
        vec![v::create(b"i", 20, b"A")?],
        v::copy_policy(&policy),
    )?)?;
    f::append(&mut log, 1, birth.as_bytes())?;
    v::accepted(v::apply(&mut state, &birth, 1)?, 1, 20, 5)?;
    match case {
        0 => policy.generation = 2,
        1 => policy.generation = 0,
        2 => policy.digest = PreparedDigest::new([0xa5; 32]),
        7 => policy.workflow_type = b"wrong".to_vec(),
        _ => {}
    }
    let operation = Operation::Transition(Transition {
        from: b"q".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(b"B".to_vec()),
    });
    let mut batch = v::batch(
        &f::scope()?,
        5,
        b"guard",
        30,
        vec![v::item(b"i", 30, operation)?],
        policy,
    )?;
    batch.inputs[0].target = if case == 5 {
        WorkflowValueTargetGuard::Absent
    } else {
        WorkflowValueTargetGuard::Present {
            incarnation: (1_u128 << 32) + u128::from(case == 6),
        }
    };
    batch.policy_references_captured = match case {
        3 | 5..=7 => None,
        4 => Some(false),
        _ => Some(true),
    };
    if case == 8 {
        batch.defaults = v::defaults(9_000);
    }
    let frame = v::encode(&batch)?;
    f::append(&mut log, 2, frame.as_bytes())?;
    log.save_committed(f::at(2))?;
    drop((log, state, frame, batch));
    let expected = match case {
        0 => WorkflowValueTransactionError::PolicyGenerationNotApplied,
        1 => WorkflowValueTransactionError::StalePolicyGeneration,
        2 => WorkflowValueTransactionError::ConflictingPolicyGeneration,
        3 => WorkflowValueTransactionError::PolicyReferenceRequired,
        4 | 8 => WorkflowValueTransactionError::InvalidInput,
        5..=7 => WorkflowValueTransactionError::StalePolicyTarget,
        _ => return Err("unknown live guard".into()),
    };
    v::refused(&dir, limits, Error::Value(expected))?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
