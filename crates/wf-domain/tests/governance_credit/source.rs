//! Exact credit traces from pinned `flow_governance_models_test.exs:149–296`.

use super::{fixtures as f, *};

#[test]
fn source_credit_owner_grants_spends_and_refuses_over_limit() -> TestResult {
    let owner = f::owner(b"tenant:acme", 10)?;
    let owner = f::grant(owner, 1, 6, 1000, 5000)?;
    assert_eq!(f::lease(&owner, 1)?.available, 6);
    assert_eq!(owner.view().free, 4);
    let owner = f::spend(owner, 1, 4, 1001, &[])?;
    assert_eq!(f::lease(&owner, 1)?.available, 2);
    assert_eq!(f::lease(&owner, 1)?.in_use, 4);
    let owner = f::grant(owner, 2, 5, 1002, 5000)?;
    assert_eq!(f::lease(&owner, 2)?.available, 4);
    assert_eq!(owner.view().free, 0);
    let (owner, result) = f::apply(
        owner,
        Op::Grant {
            shard_id: 3,
            amount: 1,
            now: TimestampMillis::new(1003),
            ttl_ms: 5000,
        },
    )?;
    let CreditDecision::Denied(denial) = result else {
        return Err("credit denial".into());
    };
    assert_eq!(denial.code(), Some("GOVERNANCE_LIMIT_EXCEEDED"));
    assert_eq!(owner.view().free, 0);
    Ok(())
}

#[test]
fn source_credit_owner_reclaims_expired_lease_credits() -> TestResult {
    let owner = f::grant(f::owner(b"tenant:acme", 10)?, 1, 6, 1000, 5)?;
    assert_eq!(owner.view().free, 4);
    let (owner, result) = f::apply(
        owner,
        Op::ReclaimExpired {
            now: TimestampMillis::new(1006),
        },
    )?;
    assert_eq!(result, CreditDecision::Reclaimed);
    assert_eq!(owner.view().free, 10);
    assert!(owner.lease(1).is_none());
    Ok(())
}

#[test]
fn source_identified_credit_release_is_idempotent_and_cannot_free_sibling() -> TestResult {
    let owner = f::grant(f::owner(b"global", 2)?, 1, 2, 1000, 1000)?;
    let owner = f::spend(owner, 1, 2, 1001, &[b"reservation-a", b"reservation-b"])?;
    let owner = f::release(owner, 1, &[b"reservation-a"])?;
    let owner = f::release(owner, 1, &[b"reservation-a"])?;
    let lease = f::lease(&owner, 1)?;
    assert_eq!(lease.in_use, 1);
    assert_eq!(lease.available, 1);
    assert_eq!(
        lease.reservations,
        [(b"reservation-b".to_vec(), Value::Amount(1))]
    );
    Ok(())
}

#[test]
fn source_credit_release_preserves_invalid_reservation_value_shape() -> TestResult {
    let mut data = f::owner(b"global", 1)?.into_data();
    let captured =
        b"\x83t\0\0\0\x02w\x06amounta\x01w\x0ereservation_idm\0\0\0\x13invalid-reservation"
            .to_vec();
    let lease = CreditLeaseData {
        shard_id: 1,
        epoch: 1,
        expires_at_ms: 2000,
        in_use: 1,
        reservations: vec![(b"invalid-reservation".to_vec(), Value::Opaque(captured))],
        ..CreditLeaseData::default()
    };
    data.free = 0;
    data.epoch = 1;
    data.leases.push(lease.clone());
    let owner = restore_credit_owner(data, CreditLimits::default())?;
    let owner = f::release(owner, 1, &[b"invalid-reservation"])?;
    assert_eq!(f::lease(&owner, 1)?.in_use, 1);
    assert_eq!(f::lease(&owner, 1)?.available, 0);
    assert_eq!(f::lease(&owner, 1)?.reservations, lease.reservations);
    Ok(())
}

#[test]
fn source_credit_owner_monotone_config_drains_without_erasing_in_use() -> TestResult {
    let owner = f::configured(b"global", 10, 1, Some(f::fingerprint(1)?))?;
    let owner = f::grant(owner, 2, 5, 1000, 10000)?;
    let owner = f::spend(
        owner,
        2,
        4,
        1001,
        &[b"shard-2-a", b"shard-2-b", b"shard-2-c", b"shard-2-d"],
    )?;
    let owner = f::grant(owner, 1, 5, 1002, 10000)?;
    let owner = f::spend(
        owner,
        1,
        4,
        1003,
        &[b"shard-1-a", b"shard-1-b", b"shard-1-c", b"shard-1-d"],
    )?;
    let owner = f::configure(owner, 6, 2, 2)?;
    assert_eq!((owner.view().limit, owner.view().config_version), (6, 2));
    assert_eq!(owner.view().policy_fingerprint, Some(f::fingerprint(2)?));
    assert_eq!(owner.view().free, 0);
    assert_eq!(
        (
            f::lease(&owner, 1)?.available,
            f::lease(&owner, 2)?.available
        ),
        (0, 0)
    );
    assert_eq!(f::lease(&owner, 1)?.in_use + f::lease(&owner, 2)?.in_use, 8);
    let (expired, _) = f::apply(
        owner.clone(),
        Op::ReclaimExpired {
            now: TimestampMillis::new(11001),
        },
    )?;
    assert_eq!(expired.view().free, 2);
    assert_eq!(f::lease(&expired, 1)?.in_use, 4);
    assert!(expired.lease(2).is_none());
    assert_eq!(f::configure(owner.clone(), 20, 1, 4)?, owner);
    let (conflict, result) = f::apply(
        owner.clone(),
        Op::Configure {
            limit: 7,
            config_version: 2,
            policy_fingerprint: Some(f::fingerprint(5)?),
        },
    )?;
    assert_eq!(
        result,
        CreditDecision::Denied(CreditRejection::ConfigurationConflict)
    );
    assert_eq!(conflict, owner);
    let owner = f::release(owner, 1, &[b"shard-1-a"])?;
    assert_eq!(
        (f::lease(&owner, 1)?.in_use, f::lease(&owner, 1)?.available),
        (3, 0)
    );
    let owner = f::release(owner, 1, &[b"shard-1-b"])?;
    assert_eq!(
        (f::lease(&owner, 1)?.in_use, f::lease(&owner, 1)?.available),
        (2, 0)
    );
    let owner = f::release(owner, 1, &[b"shard-1-c"])?;
    assert_eq!(
        (f::lease(&owner, 1)?.in_use, f::lease(&owner, 1)?.available),
        (1, 1)
    );
    let owner = f::configure(owner, 12, 3, 3)?;
    assert_eq!(
        (
            owner.view().limit,
            owner.view().config_version,
            owner.view().free
        ),
        (12, 3, 6)
    );
    assert_eq!(owner.view().policy_fingerprint, Some(f::fingerprint(3)?));
    Ok(())
}

#[test]
fn source_credit_owner_marks_reclaim_on_other_shard_exhaustion() -> TestResult {
    let owner = f::grant(f::owner(b"tenant:acme", 10)?, 1, 10, 1000, 5000)?;
    let (owner, result) = f::apply(
        owner,
        Op::Grant {
            shard_id: 2,
            amount: 1,
            now: TimestampMillis::new(1001),
            ttl_ms: 5000,
        },
    )?;
    let CreditDecision::Denied(denial) = result else {
        return Err("credit denial".into());
    };
    assert_eq!(denial.code(), Some("GOVERNANCE_LIMIT_EXCEEDED"));
    assert_eq!(f::lease(&owner, 1)?.pending_reclaim, 1);
    Ok(())
}
