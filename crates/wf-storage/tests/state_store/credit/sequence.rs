use super::{fixtures as f, *};

#[test]
fn credit_store_partial_grants_and_denials_keep_actual_owner_effects() -> TestResult {
    let dir = common::directory("credit-partial")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let first = f::execute(
        &path,
        b"first",
        (0, 1000),
        Some(f::config(3, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount: 2,
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(first.decision, Decision::Granted { shard_id: 0 });
    let first = first.owner.ok_or("missing original owner")?;
    assert_eq!(
        (
            first.view().free,
            first.lease(0).ok_or("missing lease")?.available
        ),
        (1, 2)
    );
    let second = f::execute(
        &path,
        b"second",
        (1, 1001),
        None,
        Mutation::Lease {
            shard_id: 1,
            amount: 2,
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(second.decision, Decision::Granted { shard_id: 1 });
    let owner = f::owner(&path)?;
    assert_eq!(
        (
            owner.view().free,
            owner.lease(1).ok_or("missing lease")?.available
        ),
        (0, 1)
    );
    let denied = f::execute(
        &path,
        b"denied",
        (2, 1002),
        None,
        Mutation::Lease {
            shard_id: 2,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(
        denied.decision,
        Decision::Rejected(Rejection::Credit(CreditRejection::LimitExceeded {
            requested: 1,
            free: 0,
            limit: 3
        }))
    );
    let owner = f::owner(&path)?;
    assert_eq!(owner.lease(0).ok_or("missing lease")?.pending_reclaim, 1);
    assert!(owner.lease(2).is_none());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_released_ids_replay_without_spending_a_sibling() -> TestResult {
    let dir = common::directory("credit-tombstone")?;
    let path = dir.join("state");
    f::seed(&path)?;
    let a = vec![b"a".to_vec()];
    assert_eq!(
        f::execute(
            &path,
            b"release-a",
            (2, 1002),
            None,
            Mutation::Release {
                shard_id: 0,
                amount: 1,
                reservation_ids: &a
            }
        )?
        .decision,
        Decision::Released
    );
    let before = f::owner(&path)?;
    assert_eq!(
        (
            before.lease(0).ok_or("missing lease")?.in_use,
            before.view().epoch
        ),
        (1, 1)
    );
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let duplicate = f::execute(
        &path,
        b"new-request-same-ids",
        (3, 1003),
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: Some(9000),
            reservation_ids: &ids,
        },
    )?;
    assert_eq!(duplicate.decision, Decision::Spent { shard_id: 0 });
    assert_eq!(duplicate.reservation_ids, ids);
    assert_eq!(f::owner(&path)?, before);
    let mixed = vec![b"a".to_vec(), b"c".to_vec()];
    assert_eq!(
        f::execute(
            &path,
            b"mixed",
            (4, 1004),
            None,
            Mutation::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &mixed
            }
        )?
        .decision,
        Decision::Rejected(Rejection::Credit(CreditRejection::ReservationConflict))
    );
    let missing = vec![b"c".to_vec()];
    assert_eq!(
        f::execute(
            &path,
            b"missing-release",
            (5, 1005),
            None,
            Mutation::Release {
                shard_id: 0,
                amount: 1,
                reservation_ids: &missing
            }
        )?
        .decision,
        Decision::Released
    );
    assert_eq!(f::owner(&path)?, before);
    assert_eq!(f::family_count(&path, 13)?, 2);
    assert_eq!(f::family_count(&path, 14)?, 1);
    assert_eq!(
        before.lease(0).ok_or("missing lease")?.reservations.len(),
        0
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_full_release_retires_epoch_and_old_ids_cannot_release_new_credit() -> TestResult {
    let dir = common::directory("credit-retirement")?;
    let path = dir.join("state");
    f::seed(&path)?;
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    f::execute(
        &path,
        b"release",
        (2, 1002),
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        },
    )?;
    let owner = f::owner(&path)?;
    let lease = owner.lease(0).ok_or("missing lease")?;
    assert_eq!(
        (
            owner.view().epoch,
            lease.epoch,
            lease.in_use,
            lease.reservation_page
        ),
        (2, 2, 0, 0)
    );
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (1, 1)
    );
    assert_eq!(
        f::execute(
            &path,
            b"old-spend",
            (3, 1003),
            None,
            Mutation::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &ids
            }
        )?
        .decision,
        Decision::Rejected(Rejection::LeaseGenerationChanged)
    );
    let fresh = vec![b"fresh".to_vec()];
    f::execute(
        &path,
        b"fresh-spend",
        (4, 1004),
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 2,
            amount: 1,
            ttl_ms: None,
            reservation_ids: &fresh,
        },
    )?;
    f::execute(
        &path,
        b"old-release",
        (5, 1005),
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        },
    )?;
    assert_eq!(f::owner(&path)?.lease(0).ok_or("missing lease")?.in_use, 1);
    assert_eq!(
        f::execute(&path, b"cleanup", (6, 1006), None, Mutation::Cleanup)?.decision,
        Decision::Cleaned {
            deleted: 2,
            pending: false
        }
    );
    assert_eq!(f::family_count(&path, 13)?, 1);
    assert_eq!(f::family_count(&path, 14)?, 1);
    assert_eq!(f::family_count(&path, 15)?, 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_retained_replay_precedes_changed_owner_configuration_and_clock() -> TestResult {
    let dir = common::directory("credit-retained")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let frame = f::frame(
        b"original",
        1000,
        Some(f::config(2, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount: 2,
            ttl_ms: 1000,
        },
    )?;
    let original = f::retained(&path, &frame, 0)?;
    f::execute(
        &path,
        b"reconfigure",
        (1, 1100),
        Some(f::config(5, 2)),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    let before = f::owner(&path)?;
    assert_eq!((before.view().limit, before.view().config_version), (5, 2));
    let duplicate = f::retained(&path, &frame, 2)?;
    assert_eq!(*duplicate, *original);
    assert_eq!(f::owner(&path)?, before);
    let changed = f::frame(
        b"original",
        1200,
        Some(f::config(2, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    let mut store = common::open(&path)?;
    assert!(matches!(
        apply_credit_transaction(
            &mut store,
            &common::scope()?,
            changed.as_bytes(),
            common::log(3),
            common::driver_limits(),
            CreditLimits::default()
        )?,
        wf_storage::transaction::RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(f::owner(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_expiry_cleanup_is_resumable_one_actual_page_per_command() -> TestResult {
    let dir = common::directory("credit-pages")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    f::lease(&path, 257)?;
    let ids = (0..257)
        .map(|i| format!("id-{i}").into_bytes())
        .collect::<Vec<_>>();
    assert_eq!(
        f::execute(
            &path,
            b"spend",
            (1, 1001),
            None,
            Mutation::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 257,
                ttl_ms: None,
                reservation_ids: &ids
            }
        )?
        .reservation_ids,
        ids
    );
    assert_eq!(f::family_count(&path, 13)?, 257);
    assert_eq!(f::family_count(&path, 14)?, 2);
    f::execute(&path, b"expire", (2, 2000), None, Mutation::Get)?;
    let owner = f::owner(&path)?;
    assert_eq!(owner.view().free, 257);
    assert_eq!(owner.view().leases.len(), 0);
    for (index, deleted, pending) in [(3, 256, true), (4, 1, false), (5, 0, false)] {
        assert_eq!(
            f::execute(
                &path,
                format!("cleanup-{index}").as_bytes(),
                (index, 2001 + index),
                None,
                Mutation::Cleanup
            )?
            .decision,
            Decision::Cleaned { deleted, pending }
        );
    }
    assert_eq!(f::family_count(&path, 13)?, 0);
    assert_eq!(f::family_count(&path, 14)?, 0);
    assert_eq!(f::family_count(&path, 15)?, 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_configuration_decrease_preserves_in_use_and_monotone_versions() -> TestResult {
    let dir = common::directory("credit-config")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    f::lease(&path, 10)?;
    let ids = (0..8)
        .map(|i| format!("id-{i}").into_bytes())
        .collect::<Vec<_>>();
    f::execute(
        &path,
        b"spend",
        (1, 1001),
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 8,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    f::execute(
        &path,
        b"decrease",
        (2, 1002),
        Some(f::config(6, 2)),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    let owner = f::owner(&path)?;
    let lease = owner.lease(0).ok_or("missing lease")?;
    assert_eq!(
        (
            owner.view().limit,
            owner.view().free,
            lease.in_use,
            lease.available
        ),
        (6, 0, 8, 0)
    );
    f::execute(
        &path,
        b"stale",
        (3, 1003),
        Some(f::config(20, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(f::owner(&path)?, owner);
    assert_eq!(
        f::execute(
            &path,
            b"conflict",
            (4, 1004),
            Some(f::config(7, 2)),
            Mutation::Lease {
                shard_id: 0,
                amount: 1,
                ttl_ms: 1000
            }
        )?
        .decision,
        Decision::Rejected(Rejection::Credit(CreditRejection::ConfigurationConflict))
    );
    assert_eq!(f::owner(&path)?, owner);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
