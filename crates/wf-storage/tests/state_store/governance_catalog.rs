//! Real target-side set membership, retained commands and bounded cursor reads.

use super::{TestResult, fixtures as common};
use wf_domain::governance::catalog::{CatalogMembershipAction as A, CatalogMembershipChange as C};
use wf_ports::state::StateTransactionError as S;
use wf_storage::transaction::{
    CatalogMembershipTransactionError as E, RecordBatchTransactionOutcome as Outcome,
    apply_catalog_membership_transaction,
};

mod fixtures;
mod limits;
mod progress;
mod prune;
use fixtures as f;

#[test]
fn governance_catalog_register_unregister_preserve_binary_order_and_actual_anchor() -> TestResult {
    let dir = common::directory("governance-members")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let frame = f::frame(b"add", A::Register, &[b"z", b"a\0b", b"a", b"z"])?;
    f::retained(&path, &frame, 0)?;
    assert_eq!(
        f::page(&path, None, 256)?.members,
        vec![b"a".to_vec(), b"a\0b".to_vec(), b"z".to_vec()]
    );
    let page = f::page(&path, None, 2)?;
    assert_eq!(page.next_cursor(), Some(b"a\0b".as_slice()));
    assert_eq!(
        f::page(&path, page.next_cursor(), 1)?.members,
        vec![b"z".to_vec()]
    );
    let last = f::page(&path, page.next_cursor(), 1)?;
    assert_eq!(last.next_cursor(), Some(b"z".as_slice()));
    assert_eq!(f::page(&path, last.next_cursor(), 1)?.members.len(), 0);
    let remove = f::frame(b"remove", A::Unregister, &[b"a\0b", b"", b"a\0b"])?;
    f::retained(&path, &remove, 1)?;
    let store = common::open(&path)?;
    assert_eq!(
        store
            .read_governance_catalog(
                f::read(&common::scope()?, Some(b"a\0b"), 2),
                1000,
                common::MIB
            )
            .err(),
        Some(S::CatalogChanged)
    );
    assert!(store.governance_catalog_member(
        &common::scope()?,
        b"limit",
        b"z",
        1000,
        common::MIB
    )?);
    assert!(!store.governance_catalog_member(
        &common::scope()?,
        b"other",
        b"z",
        1000,
        common::MIB
    )?);
    let other = wf_domain::workflow::request::RequestScope::new(
        wf_types::identity::TenantId::new(b"other".to_vec())?,
        common::scope()?.group().clone(),
        common::scope()?.recovery().clone(),
    );
    assert!(!store.governance_catalog_member(&other, b"limit", b"z", 1000, common::MIB)?);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_catalog_original_retry_cannot_resurrect_removed_members_and_conflicts_share_namespace()
-> TestResult {
    let dir = common::directory("governance-retry")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let add = f::frame(b"id", A::Register, &[b"a", b"b"])?;
    let original = f::retained(&path, &add, 0)?;
    let remove = f::frame(b"remove", A::Unregister, &[b"a", b"b"])?;
    f::retained(&path, &remove, 1)?;
    let replay = f::retained(&path, &add, 2)?;
    assert_eq!(replay, original);
    assert_eq!(f::page(&path, None, 256)?.members.len(), 0);
    let conflict = f::frame(b"id", A::Register, &[b"new"])?;
    let mut store = common::open(&path)?;
    assert!(matches!(
        apply_catalog_membership_transaction(
            &mut store,
            &common::scope()?,
            conflict.as_bytes(),
            common::log(3),
            common::driver_limits()
        )?,
        Outcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(f::page(&path, None, 256)?.members.len(), 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_catalog_full_256_batch_and_noop_preserve_complete_set_and_bounded_pages() -> TestResult
{
    let dir = common::directory("governance-256")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let members = (0_u16..256)
        .map(|n| n.to_be_bytes().to_vec())
        .collect::<Vec<_>>();
    let refs = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let add = f::frame(b"batch", A::Register, &refs)?;
    f::retained(&path, &add, 0)?;
    let page = f::page(&path, None, 256)?;
    assert_eq!(page.members, members);
    assert!(page.next_cursor().is_some());
    assert_eq!(f::page(&path, page.next_cursor(), 256)?.members.len(), 0);
    let before = common::raw_rows(&path)?
        .into_iter()
        .filter(|(key, _)| key.get(6) == Some(&18))
        .collect::<Vec<_>>();
    let empty = f::frame(b"empty", A::Register, &[])?;
    f::retained(&path, &empty, 1)?;
    assert_eq!(
        common::raw_rows(&path)?
            .into_iter()
            .filter(|(key, _)| key.get(6) == Some(&18))
            .collect::<Vec<_>>(),
        before
    );
    let too_many = [b"a".as_slice(); 257];
    assert!(f::frame(b"bad", A::Register, &too_many).is_err());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_catalog_receipt_only_state_refuses_a_validly_rehashed_capability_downgrade()
-> TestResult {
    let dir = common::directory("governance-downgrade")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let empty = f::frame(b"empty", A::Register, &[])?;
    f::retained(&path, &empty, 0)?;
    assert!(
        common::raw_rows(&path)?
            .keys()
            .all(|key| key.get(6) != Some(&18))
    );
    f::downgrade(&path)?;
    assert!(
        matches!(common::open(&path),Err(error)if error.downcast_ref::<S>()==Some(&S::Corrupt))
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
