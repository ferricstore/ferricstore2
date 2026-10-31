use super::super::fixtures as leaf;
use super::{fixtures as p, *};
use wf_domain::claim::caller::{ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput};
use wf_ports::log::{
    PublicClaimReplyKind as Kind, inspect_public_claim_reply, running_claim_scope,
};
use wf_storage::transaction::apply_public_claim_transaction;

#[test]
fn exhausted_whole_credit_work_or_prospective_owners_publish_nothing() -> TestResult {
    for (suffix, work, temporary) in [("work", 1, 32 * 1_048_576), ("owners", 128 * 1_048_576, 1)] {
        let dir = f::directory(&format!("public-claim-exhausted-{suffix}"))?;
        let path = dir.join("state");
        drop(p::filled(&path, true)?);
        let before = leaf::cohort(&path)?;
        let mut store = f::open(&path)?;
        let mut limits = p::limits()?;
        limits.maximum_credit_work = work;
        limits.maximum_credit_temporary_bytes = temporary;
        let frame = p::frame(true, p::BATCH)?;
        assert_eq!(
            apply_public_claim_transaction(
                &mut store,
                &f::scope()?,
                frame.as_bytes(),
                f::log(6),
                limits
            )
            .err(),
            Some(wf_storage::transaction::ClaimTransactionError::Transaction(
                wf_ports::state::StateTransactionError::Limit
            ))
        );
        drop(store);
        assert_eq!(leaf::cohort(&path)?, before);
        drop(f::open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn known_auto_lease_rejection_keeps_the_actual_new_owner_configuration() -> TestResult {
    use wf_domain::governance::{
        credit::CreditRejection, identified::IdentifiedCreditRejection as R,
    };
    use wf_domain::policy::complete::RawFlowPolicyValue as Raw;
    let dir = f::directory("public-claim-zero-limit")?;
    let path = dir.join("state");
    let mut store = p::filled(&path, false)?;
    let rule = [
        (Raw::Atom("limit"), Raw::Integer(0)),
        (Raw::Atom("lease_size"), Raw::Integer(6)),
    ];
    let running = [(Raw::Binary(b"running"), Raw::Map(&rule))];
    let governance = [(Raw::Atom("limits"), Raw::Map(&running))];
    let attrs = [
        (Raw::Atom("version"), Raw::Integer(17)),
        (Raw::Atom("governance"), Raw::Map(&governance)),
    ];
    leaf::policy(&mut store, 5, 2500, Raw::Map(&attrs))?;
    let frame = p::frame(false, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::GovernanceRejected(R::Credit(CreditRejection::LimitExceeded {
            requested: 6,
            free: 0,
            limit: 0
        }))
    );
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("actual rejected lease owner")?;
    assert_eq!(
        (
            owner.view().limit,
            owner.view().free,
            owner.view().config_version
        ),
        (0, 0, 1)
    );
    assert_eq!(owner.view().leases.len(), 0);
    assert_eq!(leaf::record(&mut store, b"q1")?.record.version(), 1);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn known_funding_failure_uses_actual_physical_presence_and_source_ratio_zero_fallback() -> TestResult
{
    for (index, state, reclaim, ratio, partition, kind) in [
        (
            0,
            b"absent".as_slice(),
            false,
            25,
            b"p".as_slice(),
            Kind::NoDemand,
        ),
        (
            1,
            b"q".as_slice(),
            false,
            25,
            b"p".as_slice(),
            Kind::FundingRejected,
        ),
        (
            2,
            b"absent".as_slice(),
            true,
            0,
            b"none".as_slice(),
            Kind::FundingRejected,
        ),
        (
            3,
            b"absent".as_slice(),
            true,
            25,
            b"p".as_slice(),
            Kind::FundingRejected,
        ),
        (
            4,
            b"absent".as_slice(),
            true,
            25,
            b"none".as_slice(),
            Kind::NoDemand,
        ),
        (
            5,
            b"running".as_slice(),
            true,
            0,
            b"none".as_slice(),
            Kind::NoDemand,
        ),
    ] {
        let dir = f::directory(&format!("public-claim-presence-{index}"))?;
        let path = dir.join("state");
        let mut store = p::filled(&path, false)?;
        let keys = [partition];
        let mut input = ClaimDueInput::new(b"r", b"new");
        input.states = ClaimStatesInput::One(state);
        input.partitions = ClaimPartitionsInput::Many(&keys);
        input.limit = Some(4);
        input.lease_ms = Some(100);
        input.reclaim_expired = Some(reclaim);
        input.reclaim_ratio = Some(ratio);
        input.governance_limit_scope = Some(b"actual-missing-owner");
        input.governance_shard_id = Some(0);
        let frame = p::encode(input, p::BATCH, 100)?;
        let outcome = leaf::retained(apply_public_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(5),
            p::limits()?,
        )?)?;
        let reply = inspect_public_claim_reply(
            outcome.response(),
            outcome.original_time(),
            v::limits().record_batch.codec.admission(),
        )?;
        assert_eq!(reply.kind(), kind, "source physical branch {index}");
        assert_eq!(
            reply.failed_credit(),
            Some(wf_domain::governance::identified::IdentifiedCreditRejection::NotFound)
        );
        assert_eq!(
            outcome.status() == RetainedStatus::Accepted,
            kind == Kind::NoDemand
        );
        assert!(reply.claim().is_none());
        assert_eq!(leaf::record(&mut store, b"q1")?.record.version(), 1);
        assert_eq!(leaf::record(&mut store, b"r1")?.record.version(), 2);
        drop(store);
        drop(f::open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn late_claim_rejection_rolls_back_claim_but_keeps_source_lease_and_refunds_all_original_ids()
-> TestResult {
    let dir = f::directory("public-claim-late-rejection")?;
    let path = dir.join("state");
    let mut store = p::filled(&path, true)?;
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(true);
    let frame = p::encode(input, p::BATCH, 1)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    assert!(matches!(outcome.status(), RetainedStatus::ClaimRejected(_)));
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("rejected claim receipt")?;
    assert_eq!((claim.count(), claim.phase_count()), (0, 0));
    assert!(claim.used_indices().next().is_none());
    assert_eq!(claim.payment().ok_or("original paid IDs")?.count(), 4);
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("source-visible owner")?;
    let lease = owner.lease(2).ok_or("source-visible lease")?;
    assert_eq!(
        (
            owner.view().limit,
            owner.view().free,
            lease.available,
            lease.in_use
        ),
        (8, 2, 6, 0)
    );
    assert_eq!(leaf::record(&mut store, b"q1")?.record.version(), 1);
    for id in [b"r1", b"r2", b"r3"] {
        assert_eq!(leaf::record(&mut store, id)?.record.version(), 2);
    }
    drop(store);
    let mut reopened = f::open(&path)?;
    assert_eq!(
        leaf::retained(apply_public_claim_transaction(
            &mut reopened,
            &f::scope()?,
            frame.as_bytes(),
            f::log(7),
            p::limits()?
        )?)?,
        outcome
    );
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
