use super::{fixtures as f, *};

#[test]
fn identified_spend_retry_precedes_changed_clock_and_does_not_extend_or_double_charge() -> TestResult
{
    let owner = f::grant(f::owner(b"scope", 4)?, 1, 4, 100, 1000)?;
    let owner = f::spend(owner, 1, 2, 101, &[b"a", b"b"])?;
    let (retry, result) = f::apply(
        owner.clone(),
        Op::Spend {
            shard_id: 1,
            amount: 2,
            now: TimestampMillis::new(90),
            ttl_ms: Some(2000),
            reservation_ids: &[b"b", b"a"],
        },
    )?;
    assert_eq!(result, CreditDecision::Spent { shard_id: 1 });
    assert_eq!(retry, owner);
    let (conflict, result) = f::apply(
        owner.clone(),
        Op::Spend {
            shard_id: 1,
            amount: 2,
            now: TimestampMillis::new(102),
            ttl_ms: None,
            reservation_ids: &[b"a", b"c"],
        },
    )?;
    assert_eq!(
        result,
        CreditDecision::Denied(CreditRejection::ReservationConflict)
    );
    assert_eq!(conflict, owner);
    let (backward, result) = f::apply(
        owner.clone(),
        Op::Spend {
            shard_id: 1,
            amount: 1,
            now: TimestampMillis::new(90),
            ttl_ms: None,
            reservation_ids: &[b"c"],
        },
    )?;
    assert_eq!(
        result,
        CreditDecision::Denied(CreditRejection::BackwardTime)
    );
    assert_eq!(backward, owner);
    let (invalid, result) = f::apply(
        owner.clone(),
        Op::Spend {
            shard_id: 1,
            amount: 2,
            now: TimestampMillis::new(102),
            ttl_ms: None,
            reservation_ids: &[b"c", b"c"],
        },
    )?;
    assert_eq!(
        result,
        CreditDecision::Denied(CreditRejection::ReservationIdsMismatch)
    );
    assert_eq!(invalid, owner);
    Ok(())
}

#[test]
fn lease_expiry_renewal_and_new_epoch_preserve_original_identity() -> TestResult {
    let owner = f::grant(f::owner(b"scope", 4)?, 7, 4, 100, 10)?;
    assert_eq!(f::lease(&owner, 7)?.epoch, 1);
    let owner = f::spend(owner, 7, 2, 101, &[b"a", b"b"])?;
    let (owner, result) = f::apply(
        owner,
        Op::Renew {
            shard_id: 7,
            now: TimestampMillis::new(105),
            ttl_ms: 30,
        },
    )?;
    assert_eq!(result, CreditDecision::Renewed { shard_id: 7 });
    assert_eq!(f::lease(&owner, 7)?.expires_at_ms, 135);
    let (expired, result) = f::apply(
        owner,
        Op::Renew {
            shard_id: 7,
            now: TimestampMillis::new(135),
            ttl_ms: 100,
        },
    )?;
    assert_eq!(
        result,
        CreditDecision::Denied(CreditRejection::LeaseExpired)
    );
    assert_eq!(expired.view().free, 4);
    assert!(expired.lease(7).is_none());
    let owner = f::grant(expired, 7, 4, 136, 10)?;
    assert_eq!(f::lease(&owner, 7)?.epoch, 2);
    assert_eq!(f::lease(&owner, 7)?.reservations, []);
    Ok(())
}

#[test]
fn invalid_or_anonymous_release_cannot_free_identified_sibling_credit() -> TestResult {
    let owner = f::grant(f::owner(b"scope", 3)?, 1, 3, 100, 1000)?;
    let owner = f::spend(owner, 1, 2, 101, &[b"a", b"b"])?;
    for ids in [
        None,
        Some([b"a".as_slice(), b"a"].as_slice()),
        Some([b"missing".as_slice()].as_slice()),
    ] {
        let amount = if ids.is_some_and(|list| list.len() == 2) {
            2
        } else {
            1
        };
        let (same, result) = f::apply(
            owner.clone(),
            Op::Release {
                shard_id: 1,
                amount,
                reservation_ids: ids,
            },
        )?;
        assert_eq!(result, CreditDecision::Released);
        assert_eq!(same, owner);
    }
    let released = f::release(owner.clone(), 1, &[b"a"])?;
    assert_eq!(f::lease(&released, 1)?.in_use, 1);
    assert_eq!(f::lease(&released, 1)?.available, 2);
    assert_eq!(f::lease(&owner, 1)?.in_use, 2);
    Ok(())
}

#[test]
fn restored_owner_and_generation_exhaustion_keep_existing_lease_usable() -> TestResult {
    let data = CreditOwnerData {
        scope: b"scope".to_vec(),
        limit: 2,
        free: 1,
        epoch: 9_007_199_254_740_991,
        config_version: 0,
        policy_fingerprint: None,
        cleanup_head: 1,
        cleanup_tail: 0,
        leases: vec![CreditLeaseData {
            shard_id: 1,
            epoch: 9_007_199_254_740_991,
            expires_at_ms: 2000,
            available: 1,
            ..CreditLeaseData::default()
        }],
    };
    let result = restore_credit_owner(data, CreditLimits::default());
    assert!(
        result.is_ok(),
        "healthy credit owner restoration: {:?}",
        result.as_ref().err()
    );
    let owner = result?;
    let (denied, result) = f::apply(
        owner.clone(),
        Op::Grant {
            shard_id: 2,
            amount: 1,
            now: TimestampMillis::new(1000),
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(
        result,
        CreditDecision::Denied(CreditRejection::GenerationExhausted)
    );
    assert_eq!(denied, owner);
    let owner = f::grant(owner, 1, 1, 1000, 1000)?;
    assert_eq!(f::lease(&owner, 1)?.available, 2);
    assert_eq!(f::lease(&owner, 1)?.epoch, 9_007_199_254_740_991);
    Ok(())
}

#[test]
fn source_configuration_without_fingerprint_is_not_an_equal_version_conflict() -> TestResult {
    let owner = f::configured(b"scope", 3, 1, Some(f::fingerprint(1)?))?;
    let (same, result) = f::apply(
        owner.clone(),
        Op::Configure {
            limit: 3,
            config_version: 1,
            policy_fingerprint: None,
        },
    )?;
    assert_eq!(result, CreditDecision::Configured);
    assert_eq!(same, owner);
    let owner = f::grant(owner, 1, 1, 100, 20)?;
    let (owner, _) = f::apply(
        owner,
        Op::ReleaseIdentifiedAmount {
            shard_id: 1,
            amount: 0,
        },
    )?;
    assert_eq!(f::lease(&owner, 1)?.available, 1);
    Ok(())
}

#[test]
fn pending_reclaim_remains_bounded_after_credits_are_spent() -> TestResult {
    let owner = f::grant(f::owner(b"scope", 2)?, 1, 2, 100, 1000)?;
    let (owner, result) = f::apply(
        owner,
        Op::Grant {
            shard_id: 2,
            amount: 1,
            now: TimestampMillis::new(101),
            ttl_ms: 1000,
        },
    )?;
    assert!(matches!(
        result,
        CreditDecision::Denied(CreditRejection::LimitExceeded { .. })
    ));
    assert_eq!(f::lease(&owner, 1)?.pending_reclaim, 1);
    let owner = f::spend(owner, 1, 2, 102, &[b"a", b"b"])?;
    let lease = f::lease(&owner, 1)?;
    assert_eq!(
        (lease.available, lease.in_use, lease.pending_reclaim),
        (0, 2, 0)
    );
    let data = owner.into_data();
    assert_eq!(
        restore_credit_owner(data.clone(), CreditLimits::default())?.view(),
        &data
    );
    Ok(())
}

#[test]
fn expired_identified_rows_free_count_slots_without_resetting_generation() -> TestResult {
    let owner = f::grant(f::owner(b"scope", 2)?, 1, 1, 100, 5)?;
    let owner = f::spend(owner, 1, 1, 101, &[b"a"])?;
    let owner = f::grant(owner, 2, 1, 102, 100)?;
    let result = decide_credit(
        owner,
        Op::Spend {
            shard_id: 2,
            amount: 1,
            now: TimestampMillis::new(106),
            ttl_ms: None,
            reservation_ids: &[b"b"],
        },
        CreditLimits::new(2, 1, 1_048_576, 1_048_576)?,
    );
    assert!(
        result.is_ok(),
        "expired reservation slot must be reusable: {:?}",
        result.as_ref().err()
    );
    let (owner, result) = result?.into_parts();
    assert_eq!(result, CreditDecision::Spent { shard_id: 2 });
    assert!(owner.lease(1).is_none());
    assert_eq!(owner.view().epoch, 2);
    assert_eq!(
        f::lease(&owner, 2)?.reservations,
        [(b"b".to_vec(), Value::Amount(1))]
    );
    assert_eq!(
        (f::lease(&owner, 2)?.available, f::lease(&owner, 2)?.in_use),
        (0, 1)
    );
    Ok(())
}

#[test]
fn bulk_identified_spend_preserves_one_thousand_ids_and_retries_without_new_owners() -> TestResult {
    let owner = f::grant(f::owner(b"scope", 1000)?, 1, 1000, 100, 1000)?;
    let bytes: Vec<_> = (0..1000)
        .rev()
        .map(|n| format!("reservation-{n:04}").into_bytes())
        .collect();
    let ids: Vec<_> = bytes.iter().map(Vec::as_slice).collect();
    let owner = f::spend(owner, 1, 1000, 101, &ids)?;
    let lease = f::lease(&owner, 1)?;
    assert_eq!(
        (lease.available, lease.in_use, lease.reservations.len()),
        (0, 1000, 1000)
    );
    let buffers: Vec<_> = lease
        .reservations
        .iter()
        .map(|(id, _)| id.as_ptr())
        .collect();
    assert!(
        lease
            .reservations
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
    );
    let (owner, result) = f::apply(
        owner,
        Op::Spend {
            shard_id: 1,
            amount: 1000,
            now: TimestampMillis::new(90),
            ttl_ms: Some(5000),
            reservation_ids: &ids,
        },
    )?;
    assert_eq!(result, CreditDecision::Spent { shard_id: 1 });
    let lease = f::lease(&owner, 1)?;
    assert_eq!(
        (lease.available, lease.in_use, lease.expires_at_ms),
        (0, 1000, 1100)
    );
    assert_eq!(
        lease
            .reservations
            .iter()
            .map(|(id, _)| id.as_ptr())
            .collect::<Vec<_>>(),
        buffers
    );
    Ok(())
}
