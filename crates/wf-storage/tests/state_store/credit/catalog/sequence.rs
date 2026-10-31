use super::{fixtures as f, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[test]
fn catalog_new_owner_and_intent_commit_together_without_replay_or_mutation_duplicates() -> TestResult
{
    let dir = common::directory("catalog-owner-atomic")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let original = f::lease(&path, b"global", 0)?;
    let page = f::page(&path, 256)?;
    assert_eq!(
        (page.meta.head(), page.meta.tail(), page.more),
        (1, 1, false)
    );
    assert_eq!(page.entries.len(), 1);
    assert_eq!(
        (
            page.entries[0].sequence,
            page.entries[0].credit_scope.as_slice()
        ),
        (1, b"global".as_slice())
    );
    let ids = vec![b"original".to_vec()];
    f::apply(
        &path,
        b"global",
        b"spend",
        1,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 1,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    let replay = f::lease(&path, b"global", 2)?;
    assert_eq!(replay.response(), original.response());
    assert_eq!(f::page(&path, 256)?, page);
    assert_eq!(credit::family_count(&path, 16)?, 1);
    assert_eq!(credit::family_count(&path, 17)?, 1);
    assert_eq!(
        credit::owner(&path)?
            .lease(0)
            .ok_or("missing lease")?
            .in_use,
        1
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_missing_owner_does_not_publish_a_ghost_but_committed_quota_denial_does() -> TestResult {
    let dir = common::directory("catalog-no-ghost")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let missing = f::apply(&path, b"global", b"missing", 0, None, Mutation::Get)?;
    assert_eq!(
        credit::reply(&missing)?.decision,
        Decision::Rejected(Rejection::NotFound)
    );
    assert_eq!(credit::family_count(&path, 16)?, 0);
    assert_eq!(credit::family_count(&path, 17)?, 0);
    assert_eq!(f::page(&path, 1)?.meta, CreditCatalogMeta::empty());
    let denied = f::apply(
        &path,
        b"global",
        b"denied",
        1,
        Some(0),
        Mutation::Lease {
            shard_id: 0,
            amount: 1,
            ttl_ms: 1000,
        },
    )?;
    assert_eq!(
        credit::reply(&denied)?.decision,
        Decision::Rejected(Rejection::Credit(CreditRejection::LimitExceeded {
            requested: 1,
            free: 0,
            limit: 0
        }))
    );
    assert_eq!(credit::owner(&path)?.view().limit, 0);
    assert_eq!(f::page(&path, 1)?.entries[0].credit_scope, b"global");
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_pages_preserve_sequence_order_more_and_tenant_isolation() -> TestResult {
    let dir = common::directory("catalog-pages")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    for (index, name) in [b"z".as_slice(), b"a", b"\0m\xff"].into_iter().enumerate() {
        f::lease(&path, name, u64::try_from(index)?)?;
    }
    let page = f::page(&path, 2)?;
    assert_eq!(
        (page.meta.head(), page.meta.tail(), page.more),
        (1, 3, true)
    );
    assert_eq!(
        page.entries
            .iter()
            .map(|entry| (entry.sequence, entry.credit_scope.as_slice()))
            .collect::<Vec<_>>(),
        [(1, b"z".as_slice()), (2, b"a".as_slice())]
    );
    let all = f::page(&path, 256)?;
    assert_eq!(all.entries.len(), 3);
    assert!(!all.more);
    assert_eq!(all.entries[2].credit_scope, b"\0m\xff");
    let store = common::open(&path)?;
    let original = common::scope()?;
    let other = wf_domain::workflow::request::RequestScope::new(
        wf_types::identity::TenantId::new(b"other".to_vec())?,
        original.group().clone(),
        original.recovery().clone(),
    );
    assert_eq!(
        store
            .read_credit_catalog(&other, 1, 10_000, 64 * common::MIB)?
            .meta,
        CreditCatalogMeta::empty()
    );
    for limit in [0, 257] {
        assert!(matches!(
            store.read_credit_catalog(&original, limit, 10_000, 64 * common::MIB),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    assert!(matches!(
        store.read_credit_catalog(&original, 1, 1, 64 * common::MIB),
        Err(StateTransactionError::Limit)
    ));
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_uncertain_creation_fences_and_recovers_a_complete_owner_intent_receipt_cohort()
-> TestResult {
    let dir = common::directory("catalog-uncertain")?;
    let path = dir.join("fault");
    let expected = dir.join("expected");
    drop(f::create(&path)?);
    drop(f::create(&expected)?);
    let before = credit::cohort(&path)?;
    assert_eq!(credit::cohort(&expected)?, before);
    let frame = credit::frame(
        b"lease",
        1000,
        Some(credit::config(3, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount: 3,
            ttl_ms: 1000,
        },
    )?;
    let after_reply = credit::retained(&expected, &frame, 0)?;
    let after = credit::cohort(&expected)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let result = apply_credit_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(0),
        common::driver_limits(),
        CreditLimits::default(),
    );
    assert!(
        matches!(
            result,
            Err(CreditTransactionError::State(
                StateTransactionError::Uncertain
            ))
        ),
        "{result:?}"
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert!(matches!(
        store.read_credit_catalog(&common::scope()?, 1, 10_000, 64 * common::MIB),
        Err(StateTransactionError::Unavailable)
    ));
    drop(store);
    drop(common::open(&path)?);
    let recovered = credit::cohort(&path)?;
    assert!(recovered == before || recovered == after);
    let reply = credit::retained(&path, &frame, u64::from(recovered == after))?;
    assert_eq!(reply.response(), after_reply.response());
    assert_eq!(f::page(&path, 256)?.entries.len(), 1);
    assert!(credit::owner(&path)?.lease(0).is_some());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
