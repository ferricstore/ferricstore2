//! Actual durable ACK sequencing, retained originals and source bounds.

use super::{fixtures as catalog, *};
use wf_domain::governance::catalog::{
    CreditCatalogAckRejection as Rejected, CreditCatalogAckReply as Reply,
    CreditCatalogAckRequest as Request,
};
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::log::{decode_catalog_ack_reply, encode_catalog_ack_request};
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_catalog_ack_transaction};

mod fixtures;
mod limits;
use fixtures as a;

#[test]
fn catalog_ack_deletes_only_original_prefix_and_replays_original_bytes_after_append() -> TestResult
{
    let dir = common::directory("catalog-ack-prefix")?;
    let path = dir.join("state");
    a::seed(&path)?;
    let owners = a::owners(&path)?;
    let outcome = a::retained(
        &path,
        b"ack",
        Request {
            expected_head: 1,
            up_to: 2,
        },
        3,
    )?;
    assert_eq!(
        a::reply(&outcome)?,
        Reply::Accepted {
            meta: CreditCatalogMeta::restore(3, 3)?,
            deleted: 2
        }
    );
    let page = catalog::page(&path, 256)?;
    assert_eq!(
        (page.meta.head(), page.meta.tail(), page.entries.len()),
        (3, 3, 1)
    );
    assert_eq!(
        (
            page.entries[0].sequence,
            page.entries[0].credit_scope.as_slice()
        ),
        (3, b"c".as_slice())
    );
    assert_eq!(a::owners(&path)?, owners);
    catalog::lease(&path, b"d", 4)?;
    let replay = a::retained(
        &path,
        b"ack",
        Request {
            expected_head: 1,
            up_to: 2,
        },
        5,
    )?;
    assert_eq!(replay.response(), outcome.response());
    assert_eq!(replay.original_position(), common::log(3));
    let page = catalog::page(&path, 256)?;
    assert_eq!(
        (page.meta.head(), page.meta.tail(), page.entries.len()),
        (3, 4, 2)
    );
    assert_eq!(page.entries[1].credit_scope, b"d");
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_retains_stale_and_invalid_results_before_changed_current_guards() -> TestResult {
    let dir = common::directory("catalog-ack-denied")?;
    let path = dir.join("state");
    a::seed(&path)?;
    let invalid = a::retained(
        &path,
        b"invalid",
        Request {
            expected_head: 1,
            up_to: 4,
        },
        3,
    )?;
    assert_eq!(
        a::reply(&invalid)?,
        Reply::Rejected {
            meta: CreditCatalogMeta::restore(1, 3)?,
            reason: Rejected::InvalidRange
        }
    );
    assert_eq!(
        invalid.status(),
        RetainedStatus::CreditCatalogRejected(Rejected::InvalidRange)
    );
    a::retained(
        &path,
        b"first",
        Request {
            expected_head: 1,
            up_to: 1,
        },
        4,
    )?;
    let stale = a::retained(
        &path,
        b"stale",
        Request {
            expected_head: 1,
            up_to: 2,
        },
        5,
    )?;
    assert_eq!(
        a::reply(&stale)?,
        Reply::Rejected {
            meta: CreditCatalogMeta::restore(2, 3)?,
            reason: Rejected::StaleCursor
        }
    );
    assert_eq!(
        stale.status(),
        RetainedStatus::CreditCatalogRejected(Rejected::StaleCursor)
    );
    a::retained(
        &path,
        b"remaining",
        Request {
            expected_head: 2,
            up_to: 3,
        },
        6,
    )?;
    let replay = a::retained(
        &path,
        b"stale",
        Request {
            expected_head: 1,
            up_to: 2,
        },
        7,
    )?;
    assert_eq!(replay.response(), stale.response());
    let noop = a::retained(
        &path,
        b"already",
        Request {
            expected_head: 1,
            up_to: 1,
        },
        8,
    )?;
    assert_eq!(
        a::reply(&noop)?,
        Reply::Accepted {
            meta: CreditCatalogMeta::restore(4, 3)?,
            deleted: 0
        }
    );
    assert_eq!(
        catalog::page(&path, 256)?.meta,
        CreditCatalogMeta::restore(4, 3)?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_shares_the_owner_request_namespace_in_both_directions() -> TestResult {
    let dir = common::directory("catalog-ack-namespace")?;
    let path = dir.join("state");
    a::seed(&path)?;
    let original = catalog::page(&path, 256)?;
    assert!(matches!(
        a::run(
            &path,
            b"a",
            Request {
                expected_head: 1,
                up_to: 1
            },
            3,
            common::driver_limits()
        )?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    assert_eq!(catalog::page(&path, 256)?, original);
    a::retained(
        &path,
        b"ack",
        Request {
            expected_head: 1,
            up_to: 1,
        },
        4,
    )?;
    let frame = encode_credit_request(
        &common::scope()?,
        &wf_types::identity::RequestId::new(b"ack".to_vec())?,
        common::clock(1001)?,
        IdentifiedCreditRequest {
            scope: b"new",
            shard_count: 4,
            configuration: Some(credit::config(3, 1)),
            mutation: Mutation::Lease {
                shard_id: 0,
                amount: 3,
                ttl_ms: 1000,
            },
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    let before = catalog::page(&path, 256)?;
    let mut store = common::open(&path)?;
    assert!(matches!(
        apply_credit_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(5),
            common::driver_limits(),
            CreditLimits::default()
        )?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(catalog::page(&path, 256)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_256_entry_page_is_independent_of_workflow_batch_limit() -> TestResult {
    let dir = common::directory("catalog-ack-256")?;
    let path = dir.join("state");
    drop(catalog::create(&path)?);
    catalog::lease(&path, b"global", 0)?;
    a::entries(&path, 257)?;
    drop(common::open(&path)?);
    let mut limits = common::driver_limits();
    let original = limits.codec.admission();
    limits.codec =
        wf_ports::log::CodecLimits::new(wf_domain::workflow::admission::AdmissionLimits::new(
            original.inline_bytes(),
            original.decision_bytes(),
            1,
        )?);
    let outcome = a::run(
        &path,
        b"page",
        Request {
            expected_head: 1,
            up_to: 256,
        },
        1,
        limits,
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
        return Err("missing retained page ACK".into());
    };
    assert_eq!(
        a::reply(&outcome)?,
        Reply::Accepted {
            meta: CreditCatalogMeta::restore(257, 257)?,
            deleted: 256
        }
    );
    let page = catalog::page(&path, 256)?;
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].sequence, 257);
    catalog::lease(&path, b"next", 2)?;
    assert_eq!(
        catalog::page(&path, 256)?.meta,
        CreditCatalogMeta::restore(257, 258)?
    );
    assert!(
        a::frame(
            b"too-many",
            Request {
                expected_head: 1,
                up_to: 257
            }
        )
        .is_err()
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_receipt_only_rejection_requires_the_actual_catalog_capability() -> TestResult {
    let dir = common::directory("catalog-ack-receipt-capability")?;
    let path = dir.join("state");
    drop(catalog::create(&path)?);
    let outcome = a::retained(
        &path,
        b"empty",
        Request {
            expected_head: 1,
            up_to: 1,
        },
        0,
    )?;
    assert_eq!(
        a::reply(&outcome)?,
        Reply::Rejected {
            meta: CreditCatalogMeta::empty(),
            reason: Rejected::InvalidRange
        }
    );
    for family in [12, 16, 17] {
        assert_eq!(credit::family_count(&path, family)?, 0);
    }
    catalog::downgrade_catalog(&path)?;
    assert!(
        matches!(common::open(&path),Err(error) if error.downcast_ref::<StateTransactionError>()==Some(&StateTransactionError::Corrupt))
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
