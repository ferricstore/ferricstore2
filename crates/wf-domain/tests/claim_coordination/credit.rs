use super::{TestResult, caller, credit_batch};
use wf_domain::claim::caller::ClaimDueInput;
use wf_domain::claim::coordination::{
    ClaimCoordinationError, ClaimCoordinationLimits, ClaimCreditUse, ClaimExecutionOutcome,
    ClaimSpendOutcome, CreditSettlementDisposition, admit_claim_credit_batch, classify_claim_spend,
    plan_claim_credit_settlement, resolve_running_credit,
};
use wf_domain::policy::installation::PolicyLimits;
use wf_types::version::TimestampMillis;

#[test]
fn identified_batch_checks_all_occurrences_before_ownership() -> TestResult {
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(3);
    input.governance_limit_scope = Some(b"s");
    input.governance_shard_id = Some(0);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        None,
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing demand")?;
    let owners = [b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
    let ids: Vec<&[u8]> = owners.iter().map(Vec::as_slice).collect();
    let batch = admit_claim_credit_batch(
        &demand,
        b"s",
        0,
        1,
        &ids,
        ClaimCoordinationLimits::new(1, 3, 390)?,
    )?;
    assert_eq!((batch.shard_id(), batch.epoch()), (0, 1));
    assert!(batch.enforcement() == demand.enforcement());
    for (actual, original) in batch.reservation_ids().iter().zip(&owners) {
        assert!(std::ptr::eq(*actual, original.as_slice()));
    }
    assert!(matches!(
        admit_claim_credit_batch(
            &demand,
            b"s",
            0,
            1,
            &ids,
            ClaimCoordinationLimits::new(1, 3, 389)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    let duplicate: [&[u8]; 3] = [b"a", b"b", b"a"];
    let extra = vec![b'x'; 257];
    let bad: [&[u8]; 3] = [b"a", b"b", &extra];
    let empty: [&[u8]; 3] = [b"a", b"b", b""];
    for (shard, candidates) in [
        (0, duplicate.as_slice()),
        (0, bad.as_slice()),
        (0, empty.as_slice()),
        (u64::MAX, ids.as_slice()),
    ] {
        assert!(matches!(
            admit_claim_credit_batch(
                &demand,
                b"s",
                shard,
                1,
                candidates,
                ClaimCoordinationLimits::default()
            ),
            Err(ClaimCoordinationError::InvalidCreditAuthority)
        ));
    }
    let scope = vec![b's'; 65_535];
    input.limit = Some(1);
    input.governance_limit_scope = Some(&scope);
    let wide_call = caller(input)?;
    let wide = resolve_running_credit(
        &wide_call,
        None,
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing wide demand")?;
    let id = vec![b'i'; 256];
    let one: [&[u8]; 1] = [&id];
    assert!(credit_batch(&wide, &scope, &one).is_ok());
    let over_scope = vec![b's'; 65_536];
    assert!(credit_batch(&wide, &over_scope, &one).is_err());
    Ok(())
}

#[test]
fn refund_uses_actual_nonprefix_ids_in_original_order() -> TestResult {
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(3);
    input.governance_limit_scope = Some(b"s");
    input.governance_shard_id = Some(0);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        None,
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing demand")?;
    let ids: [&[u8]; 3] = [b"a", b"b", b"c"];
    let batch = credit_batch(&demand, b"s", &ids)?;
    let used: [&[u8]; 4] = [b"b", b"b", b"foreign", b"c"];
    let settled = plan_claim_credit_settlement(
        &batch,
        ClaimExecutionOutcome::Accepted(&used),
        ClaimCoordinationLimits::default(),
    )?;
    assert!(settled.disposition() == CreditSettlementDisposition::RefundUnused);
    assert!(std::ptr::eq(settled.batch(), &raw const batch));
    assert_eq!(settled.refund_ids(), [b"a".as_slice()]);
    assert!(std::ptr::eq(settled.refund_ids()[0], ids[0]));
    let used: [&[u8]; 1] = [b"b"];
    assert_eq!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::Accepted(&used),
            ClaimCoordinationLimits::new(1, 3, 373)?
        )?
        .refund_ids(),
        [b"a".as_slice(), b"c".as_slice()]
    );
    assert!(matches!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::Accepted(&used),
            ClaimCoordinationLimits::new(1, 3, 372)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    assert_eq!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::Accepted(&[]),
            ClaimCoordinationLimits::default()
        )?
        .refund_ids(),
        ids
    );
    let many = vec![b"b".as_slice(); 1000];
    assert!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::Accepted(&many),
            ClaimCoordinationLimits::default()
        )
        .is_ok()
    );
    let too_many = vec![b"b".as_slice(); 1001];
    assert!(matches!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::Accepted(&too_many),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidCreditAuthority)
    ));
    let huge = vec![b'x'; 257];
    let bad: [&[u8]; 2] = [b"b", &huge];
    assert!(matches!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::Accepted(&bad),
            ClaimCoordinationLimits::default()
        ),
        Err(ClaimCoordinationError::InvalidCreditAuthority)
    ));
    Ok(())
}

#[test]
fn unknown_spend_and_unknown_claim_keep_exact_ids() -> TestResult {
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.governance_limit_scope = Some(b"scope");
    input.governance_shard_id = Some(0);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        None,
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing demand")?;
    let owner = b"original".to_vec();
    let ids = [owner.as_slice()];
    let batch = credit_batch(&demand, b"scope", &ids)?;
    assert!(
        matches!(classify_claim_spend(ClaimSpendOutcome::Unknown(&batch)),
        ClaimCreditUse::ResolveUnknown(original) if std::ptr::eq(original, &raw const batch))
    );
    assert!(
        matches!(classify_claim_spend(ClaimSpendOutcome::Confirmed(&batch)),
        ClaimCreditUse::Proceed(original) if std::ptr::eq(original, &raw const batch))
    );
    assert!(matches!(
        classify_claim_spend(ClaimSpendOutcome::KnownFailure),
        ClaimCreditUse::Reject
    ));
    for (outcome, expected) in [
        (
            ClaimExecutionOutcome::UnknownCommit,
            CreditSettlementDisposition::HoldUnknown,
        ),
        (
            ClaimExecutionOutcome::Unclassified,
            CreditSettlementDisposition::HoldUnclassified,
        ),
    ] {
        let settled =
            plan_claim_credit_settlement(&batch, outcome, ClaimCoordinationLimits::default())?;
        assert!(settled.disposition() == expected);
        assert_eq!(settled.refund_ids(), &[] as &[&[u8]]);
        assert!(std::ptr::eq(
            settled.batch().reservation_ids()[0],
            owner.as_slice()
        ));
    }
    assert!(matches!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::UnknownCommit,
            ClaimCoordinationLimits::new(1, 1, 1)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    Ok(())
}

#[test]
fn known_no_commit_refund_needs_no_fabricated_empty_proof() -> TestResult {
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(3);
    input.governance_limit_scope = Some(b"s");
    input.governance_shard_id = Some(0);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        None,
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing demand")?;
    let ids: [&[u8]; 3] = [b"a", b"b", b"c"];
    let batch = credit_batch(&demand, b"s", &ids)?;
    let settled = plan_claim_credit_settlement(
        &batch,
        ClaimExecutionOutcome::KnownNoCommit,
        ClaimCoordinationLimits::new(1, 3, 244)?,
    )?;
    assert!(settled.disposition() == CreditSettlementDisposition::RefundNoCommit);
    assert_eq!(settled.refund_ids(), ids);
    assert!(std::ptr::eq(settled.batch(), &raw const batch));
    for (actual, original) in settled.refund_ids().iter().zip(ids) {
        assert!(std::ptr::eq(*actual, original));
    }
    assert!(matches!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::KnownNoCommit,
            ClaimCoordinationLimits::new(1, 3, 243)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    assert!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::UnknownCommit,
            ClaimCoordinationLimits::new(1, 3, 196)?
        )
        .is_ok()
    );
    assert!(matches!(
        plan_claim_credit_settlement(
            &batch,
            ClaimExecutionOutcome::UnknownCommit,
            ClaimCoordinationLimits::new(1, 3, 195)?
        ),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(1000);
    input.governance_limit_scope = Some(b"s");
    input.governance_shard_id = Some(0);
    let call = caller(input)?;
    let demand = resolve_running_credit(
        &call,
        None,
        TimestampMillis::new(0),
        4,
        PolicyLimits::default(),
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("missing large demand")?;
    let owners: Vec<[u8; 8]> = (0_u64..1000).map(u64::to_be_bytes).collect();
    let ids: Vec<&[u8]> = owners.iter().map(<[u8; 8]>::as_slice).collect();
    let batch = credit_batch(&demand, b"s", &ids)?;
    let settled = plan_claim_credit_settlement(
        &batch,
        ClaimExecutionOutcome::KnownNoCommit,
        ClaimCoordinationLimits::default(),
    )?;
    assert_eq!(settled.refund_ids().len(), 1000);
    assert_eq!(settled.refund_ids(), ids.as_slice());
    assert!(std::ptr::eq(settled.batch(), &raw const batch));
    for (actual, original) in settled.refund_ids().iter().zip(&ids) {
        assert!(std::ptr::eq(*actual, *original));
    }
    input.limit = Some(1001);
    assert!(
        resolve_running_credit(
            &caller(input)?,
            None,
            TimestampMillis::new(0),
            4,
            PolicyLimits::default(),
            ClaimCoordinationLimits::default()
        )
        .is_err()
    );
    Ok(())
}
