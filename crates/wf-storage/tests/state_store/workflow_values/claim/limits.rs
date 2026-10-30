use super::{fixtures as s, *};
use wf_domain::policy::complete::RawFlowPolicyValue as Raw;
use wf_ports::state::StateTransactionError;
use wf_storage::transaction::{
    ClaimTransactionError, ClaimTransactionLimits, apply_claim_transaction,
};

#[test]
fn named_value_claim_caps_and_unupgraded_or_legacy_drivers_preserve_cohorts() -> TestResult {
    let dir = f::directory("named-value-claim-caps")?;
    let healthy = dir.join("healthy");
    let mut ready = s::fresh(&healthy, false)?;
    s::create(&mut ready, b"i", b"q", (0, 0, 0), Some(b"A"))?;
    s::retained(s::apply(
        &mut ready,
        &s::phase(b"claim", 2, b"q", b"w", 1)?,
        1,
    )?)?;
    drop(ready);
    let path = dir.join("state");
    let mut store = s::fresh(&path, false)?;
    s::create(&mut store, b"i", b"q", (0, 0, 0), Some(b"A"))?;
    drop(store);
    let before = s::cohort(&path)?;
    let frame = s::phase(b"claim", 2, b"q", b"w", 1)?;
    for case in 0..8 {
        let mut limits = s::limits();
        match case {
            0 => limits.maximum_value_rows = 0,
            1 => limits.maximum_value_rows = 1,
            2 => limits.maximum_value_bytes = 1,
            3 => limits.maximum_value_work = 1,
            4 => limits.maximum_value_temporary_bytes = 1,
            5 => limits.record_batch.maximum_read_steps = 1,
            6 => limits.record_batch.maximum_adapter_bytes = 1,
            _ => limits.record_batch.workspace_bytes = 1,
        }
        let mut store = f::open(&path)?;
        assert!(
            apply_workflow_value_claim_transaction(
                &mut store,
                &f::scope()?,
                frame.as_bytes(),
                f::log(1),
                limits,
                ClaimWorkLimits::default()
            )
            .is_err(),
            "case {case}"
        );
        drop(store);
        assert_eq!(s::cohort(&path)?, before);
    }
    let mut store = f::open(&path)?;
    assert_eq!(
        apply_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(1),
            ClaimTransactionLimits {
                base: s::limits().record_batch,
                work: ClaimWorkLimits::default()
            }
        )
        .err(),
        Some(ClaimTransactionError::Transaction(
            StateTransactionError::UnsupportedLayout
        ))
    );
    drop(store);
    assert_eq!(s::cohort(&path)?, before);
    let legacy = dir.join("legacy");
    let mut store = v::fresh(&legacy)??;
    s::create(&mut store, b"i", b"q", (0, 0, 0), None)?;
    drop(store);
    let before = s::cohort(&legacy)?;
    let mut store = f::open(&legacy)?;
    assert_eq!(
        apply_workflow_value_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(1),
            s::limits(),
            ClaimWorkLimits::default()
        )
        .err(),
        Some(ClaimTransactionError::Transaction(
            StateTransactionError::UnsupportedLayout
        ))
    );
    drop(store);
    assert_eq!(s::cohort(&legacy)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn named_value_claim_replay_precedes_current_governance_but_fresh_does_not_bypass_it() -> TestResult
{
    let dir = f::directory("named-value-claim-governance")?;
    let path = dir.join("state");
    let mut store = s::fresh(&path, false)?;
    s::create(&mut store, b"i", b"q", (0, 0, 0), None)?;
    let original = s::retained(s::apply(
        &mut store,
        &s::phase(b"claim", 2, b"q", b"w", 1)?,
        1,
    )?)?;
    let attrs = [(Raw::Atom("governance"), Raw::Map(&[]))];
    s::policy(&mut store, 2, 3, Raw::Map(&attrs))?;
    let replay = s::retained(s::apply(
        &mut store,
        &s::phase(b"claim", 4, b"q", b"w", 1)?,
        3,
    )?)?;
    assert_eq!(replay.as_ref(), original.as_ref());
    drop(store);
    let before = s::cohort(&path)?;
    let mut store = f::open(&path)?;
    let fresh = s::phase(b"fresh", 5, b"q", b"w", 1)?;
    assert_eq!(
        apply_workflow_value_claim_transaction(
            &mut store,
            &f::scope()?,
            fresh.as_bytes(),
            f::log(4),
            s::limits(),
            ClaimWorkLimits::default()
        )
        .err(),
        Some(ClaimTransactionError::UnsupportedGovernance)
    );
    drop(store);
    assert_eq!(s::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
