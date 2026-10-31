use super::{TestResult, encode, intent, options};
use wf_domain::claim::caller::ClaimCallerLimits;
use wf_domain::{claim::caller::ClaimDueInput, workflow::active::LimitEnforcement};
use wf_ports::log::GovernedClaimPayment;
use wf_ports::log::{CodecError, prepare_atomic_claim_request};

#[test]
fn committed_time_and_operational_budgets_leave_original_intent_unchanged() -> TestResult {
    let caller = ClaimDueInput::new(b"job", b"worker");
    let baseline = encode(caller, options(), 100)?;
    let mut changed = options();
    changed.first_ordinal = 99;
    changed.scan_budget = 3;
    changed.head_budget = 2;
    let other = encode(caller, changed, 500)?;
    assert_eq!(intent(baseline.as_bytes())?, intent(other.as_bytes())?);
    assert_ne!(baseline.as_bytes(), other.as_bytes());
    Ok(())
}

#[test]
fn original_generated_payment_ids_and_epoch_affect_only_prepared_identity() -> TestResult {
    let a = [b"id\0".to_vec()];
    let b = [b"other".to_vec()];
    let mut left = options();
    left.payment = Some(GovernedClaimPayment {
        scope: b"paid",
        shard_id: 0,
        epoch: 1,
        enforcement: LimitEnforcement::StrictGlobal,
        reservation_ids: &a,
    });
    let mut right = left;
    right.payment = Some(GovernedClaimPayment {
        epoch: 2,
        reservation_ids: &b,
        ..left.payment.ok_or("payment")?
    });
    let first = encode(ClaimDueInput::new(b"job", b"worker"), left, 100)?;
    let second = encode(ClaimDueInput::new(b"job", b"worker"), right, 100)?;
    assert_eq!(intent(first.as_bytes())?, intent(second.as_bytes())?);
    assert_ne!(first.as_bytes(), second.as_bytes());
    assert!(first.as_bytes().windows(3).any(|bytes| bytes == b"id\0"));
    Ok(())
}

#[test]
fn complete_parent_is_decoded_before_any_truncated_suffix_is_accepted() -> TestResult {
    let frame = encode(ClaimDueInput::new(b"job", b"worker"), options(), 100)?;
    let ready = prepare_atomic_claim_request(
        frame.as_bytes(),
        super::limits(),
        ClaimCallerLimits::default(),
        1024 * 1024,
    )?;
    assert_eq!(ready.as_bytes(), frame.as_bytes());
    for cut in 0..frame.as_bytes().len() {
        assert!(
            prepare_atomic_claim_request(
                &frame.as_bytes()[..cut],
                super::limits(),
                ClaimCallerLimits::default(),
                1024 * 1024
            )
            .is_err(),
            "cut {cut}"
        );
    }
    Ok(())
}

#[test]
fn parent_owner_bound_is_checked_before_reference_ownership() -> TestResult {
    let frame = encode(ClaimDueInput::new(b"job", b"worker"), options(), 100)?;
    assert_eq!(
        prepare_atomic_claim_request(
            frame.as_bytes(),
            super::limits(),
            ClaimCallerLimits::default(),
            1
        )
        .err(),
        Some(CodecError::OwnedLimit)
    );
    Ok(())
}
