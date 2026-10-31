//! Actual empty-profile refusals and explicit credit owner-budget enforcement.
use super::*;
use super::{fixtures as p, frames as q};
use wf_node::recovery::recover_committed;
use wf_ports::state::StateTransactionError as S;
use wf_storage::transaction::apply_governed_claim_transaction_with_credit_limits;

#[test]
fn release_node_older_profiles_refuse_actual_empty_gc4_and_gc5_without_writes() -> TestResult {
    let s = f::scope()?;
    for paid in [false, true] {
        let dir = f::directory("release-node-empty-profile")?;
        drop(p::pair(&dir, paid)?);
        let before = p::cohort(&dir)?;
        let logs = f::log_rows(&dir)?;
        for limits in [
            caps()?,
            caps()?.with_governance_catalogs(),
            caps()?.with_governance_catalog_progress(),
            caps()?.with_governance_catalog_pruning(),
            p::selected(!paid)?,
        ] {
            let (log, state) = f::open_pair(&dir)?;
            assert_eq!(
                recover_committed(log, state, s.group(), s.recovery(), limits).err(),
                Some(E::State(S::UnsupportedPolicyProfile))
            );
            assert_eq!(p::cohort(&dir)?, before);
            assert_eq!(f::log_rows(&dir)?, logs);
        }
        let (log, state) = f::open_pair(&dir)?;
        drop(recover_committed(
            log,
            state,
            s.group(),
            s.recovery(),
            p::selected(paid)?,
        )?);
        assert_eq!(p::cohort(&dir)?, before);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn release_node_revalidates_credit_dependencies_after_later_builder_selection() -> TestResult {
    let dir = f::directory("release-node-weakened-deps")?;
    drop(p::pair(&dir, true)?);
    let s = f::scope()?;
    let before = p::cohort(&dir)?;
    let limits = p::selected(true)?.with_identified_credit_limits(4, CreditLimits::default())?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, s.group(), s.recovery(), limits).err(),
        Some(E::InvalidLimits)
    );
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn paid_claim_explicit_credit_owner_budget_refuses_without_mutation() -> TestResult {
    let dir = f::directory("release-node-credit-owner-budget")?;
    let (log, mut state) = p::pair(&dir, true)?;
    let frames = q::all()?;
    p::apply(&mut state, &frames, 4)?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let (log, mut state) = f::open_pair(&dir)?;
    let s = f::scope()?;
    let small = CreditLimits::new(1000, 1000, 1, CreditLimits::default().max_work())?;
    assert_eq!(
        apply_governed_claim_transaction_with_credit_limits(
            &mut state,
            &s,
            frames[4].as_bytes(),
            f::at(4),
            p::values()?,
            ClaimWorkLimits::default(),
            small
        )
        .err(),
        Some(wf_storage::transaction::ClaimTransactionError::Transaction(
            S::Limit
        ))
    );
    drop((log, state));
    assert_eq!(p::cohort(&dir)?, before);
    let (log, mut state) = f::open_pair(&dir)?;
    let result = apply_governed_claim_transaction_with_credit_limits(
        &mut state,
        &s,
        frames[4].as_bytes(),
        f::at(4),
        p::values()?,
        ClaimWorkLimits::default(),
        CreditLimits::default(),
    )?;
    assert!(matches!(
        result,
        wf_storage::transaction::ClaimTransactionOutcome::Retained { .. }
    ));
    assert_eq!(p::record(&mut state, b"i")?.record.state(), b"running");
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
