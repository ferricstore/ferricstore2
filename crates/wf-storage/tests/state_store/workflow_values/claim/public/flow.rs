use super::super::fixtures as leaf;
use super::{fixtures as p, *};
use wf_ports::log::{
    CreditSpendPreparation, PublicClaimReplyKind, generate_public_credit_ids,
    inspect_public_claim_reply, running_claim_scope,
};
use wf_storage::transaction::apply_public_claim_transaction;

#[test]
fn ungoverned_public_parent_runs_all_phases_without_creating_credit() -> TestResult {
    let dir = f::directory("public-claim-no-governance")?;
    let path = dir.join("state");
    let mut store = p::filled(&path, false)?;
    let frame = p::frame(true, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        p::limits()?,
    )?)?;
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (21, 18)
    );
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("actual claim")?;
    assert_eq!(
        (reply.kind(), claim.count(), claim.phase_count()),
        (PublicClaimReplyKind::Claim, 4, 3)
    );
    assert!(claim.payment().is_none());
    assert!(
        store
            .read_credit_owner(
                &f::scope()?,
                &running_claim_scope(b"r"),
                p::limits()?.base.credit,
                10000,
                16 * 1_048_576
            )?
            .is_none()
    );
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn actual_current_rule_spends_then_leases_then_spends_before_claim_in_one_commit() -> TestResult {
    let dir = f::directory("public-claim-atomic-funding")?;
    let path = dir.join("state");
    let mut store = p::filled(&path, true)?;
    let frame = p::frame(true, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("actual claim")?;
    assert_eq!((claim.count(), claim.phase_count()), (4, 3));
    let paid = claim.payment().ok_or("actual paid batch")?;
    let original = generate_public_credit_ids(
        4,
        CreditSpendPreparation {
            lease_epoch: 1,
            batch: p::BATCH,
        },
        1_048_576,
    )?;
    assert_eq!(
        paid.reservation_ids().collect::<Result<Vec<_>, _>>()?,
        original.iter().map(Vec::as_slice).collect::<Vec<_>>()
    );
    assert_eq!(
        (paid.scope(), paid.shard_id(), paid.epoch()),
        (running_claim_scope(b"r").as_slice(), 2, 1)
    );
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("actual credit owner")?;
    let lease = owner.lease(2).ok_or("lease")?;
    assert_eq!(
        (
            owner.view().limit,
            owner.view().free,
            lease.available,
            lease.in_use
        ),
        (8, 2, 2, 4)
    );
    drop(store);
    let mut reopened = f::open(&path)?;
    let changed = p::frame(true, [0x99; 16])?;
    assert_eq!(
        leaf::retained(apply_public_claim_transaction(
            &mut reopened,
            &f::scope()?,
            changed.as_bytes(),
            f::log(7),
            p::limits()?
        )?)?,
        outcome
    );
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn only_original_unused_ids_are_refunded_before_the_single_parent_commit() -> TestResult {
    let dir = f::directory("public-claim-unused-refund")?;
    let path = dir.join("state");
    let mut store = p::filled(&path, true)?;
    let frame = p::frame(false, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("actual claim")?;
    assert_eq!(claim.count(), 1);
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [0]);
    assert_eq!(claim.payment().ok_or("original batch")?.count(), 4);
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.lease(2).ok_or("lease")?.available,
            owner.lease(2).ok_or("lease")?.in_use
        ),
        (5, 1)
    );
    assert_eq!(leaf::record(&mut store, b"r1")?.record.version(), 2);
    assert_eq!(leaf::record(&mut store, b"q1")?.record.version(), 2);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
