//! Actual GC4/5 startup selection and complete native body custody.
use super::{TestResult, caps, f};
use wf_domain::{claim::apply::ClaimWorkLimits, governance::credit::CreditLimits};
use wf_node::recovery::{CommittedRecoveryError as E, CommittedRecoveryLimits};
mod atomic_claim;
mod fixtures;
mod frames;
mod history;
mod limits;
mod reconcile;
mod snapshot;

fn stored() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?.with_workflow_stored_value_limits(1024, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB)?)
}

#[test]
fn release_node_requires_explicit_stored_credit_and_claim_dependencies() -> TestResult {
    assert!(matches!(
        caps()?.with_workflow_release_recovery(),
        Err(E::InvalidLimits)
    ));
    assert!(matches!(
        stored()?.with_workflow_release_recovery(),
        Err(E::InvalidLimits)
    ));
    assert!(matches!(
        stored()?
            .with_identified_credit_limits(4, CreditLimits::default())?
            .with_workflow_release_recovery(),
        Err(E::InvalidLimits)
    ));
    let release = stored()?.with_identified_credit_catalog_limits(4, CreditLimits::default())?;
    assert!(release.with_workflow_release_recovery().is_ok());
    assert!(matches!(
        release.with_governed_claim_recovery(),
        Err(E::InvalidLimits)
    ));
    assert!(
        release
            .with_workflow_value_claim_limits(
                ClaimWorkLimits::default(),
                1024,
                64 * f::MIB,
                64 * f::MIB,
                32 * f::MIB
            )?
            .with_governed_claim_recovery()
            .is_ok()
    );
    Ok(())
}
