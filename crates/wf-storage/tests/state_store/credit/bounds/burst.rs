//! Actual broad-shard donors prove bounded expiry progress and exact capacity.

use super::*;
use wf_domain::governance::credit::{CreditLeaseData, CreditOwnerData};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::RequestId;

#[test]
fn credit_store_cleanup_batches_more_than_256_new_expired_generations() -> TestResult {
    let dir = common::directory("credit-expiry-burst")?;
    let path = dir.join("state");
    setup(&path, false)?;
    let before = business(&path)?;
    let refused = execute(&path, b"ordinary-refusal", 1, Mutation::Get)?;
    assert_eq!(
        refused.decision,
        Decision::Rejected(Rejection::CleanupBacklogFull)
    );
    assert_eq!(business(&path)?, before);
    let first = execute(&path, b"first-cleanup", 2, Mutation::Cleanup)?;
    assert_eq!(
        first.decision,
        Decision::Cleaned {
            deleted: 1,
            pending: true
        }
    );
    let owner = f::owner(&path)?;
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (2, 256)
    );
    assert_eq!(owner.view().free, 512);
    assert_eq!(owner.view().leases.len(), 1);
    let held = owner
        .lease(256)
        .ok_or("last expired generation reclaimed without a queue slot")?;
    assert_eq!(
        (held.epoch, held.available, held.in_use, held.expires_at_ms),
        (257, 1, 1, 2000)
    );
    assert_eq!(f::family_count(&path, 13)?, 256);
    assert_eq!(f::family_count(&path, 15)?, 255);
    drop(common::open(&path)?);
    let next = execute(&path, b"last-expiry", 3, Mutation::Get)?;
    assert_eq!(next.decision, Decision::Owner);
    let owner = next.owner.ok_or("missing final reclaimed owner")?;
    assert_eq!(owner.view().free, 514);
    assert_eq!(owner.view().leases.len(), 0);
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (2, 257)
    );
    let next = execute(&path, b"next-cleanup", 4, Mutation::Cleanup)?;
    assert_eq!(
        next.decision,
        Decision::Cleaned {
            deleted: 1,
            pending: true
        }
    );
    assert_eq!(f::family_count(&path, 13)?, 255);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_burst_keeps_live_leases_and_reclaims_only_safe_expired_capacity() -> TestResult {
    let dir = common::directory("credit-mixed-expiry-burst")?;
    let path = dir.join("state");
    setup(&path, true)?;
    let first = execute(&path, b"mixed-cleanup", 1, Mutation::Cleanup)?;
    assert_eq!(
        first.decision,
        Decision::Cleaned {
            deleted: 1,
            pending: true
        }
    );
    let owner = f::owner(&path)?;
    assert_eq!(owner.view().free, 514);
    assert_eq!(owner.view().leases.len(), 3);
    assert_eq!(
        owner
            .lease(0)
            .ok_or("live identified lease lost")?
            .expires_at_ms,
        5000
    );
    assert_eq!(
        owner
            .lease(257)
            .ok_or("unqueued expired lease lost")?
            .in_use,
        1
    );
    assert!(
        owner.lease(258).is_none(),
        "page-free expired capacity was not reclaimed"
    );
    assert_eq!(
        owner
            .lease(259)
            .ok_or("page-free live lease lost")?
            .available,
        2
    );
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (2, 256)
    );
    assert_eq!(f::family_count(&path, 13)?, 257);
    drop(common::open(&path)?);
    let next = execute(&path, b"remaining-mixed-expiry", 2, Mutation::Get)?;
    let owner = next.owner.ok_or("missing mixed owner")?;
    assert_eq!(owner.view().free, 516);
    assert_eq!(owner.view().leases.len(), 2);
    assert!(owner.lease(257).is_none());
    assert!(owner.lease(0).is_some());
    assert!(owner.lease(259).is_some());
    assert_eq!(
        (owner.view().cleanup_head, owner.view().cleanup_tail),
        (2, 257)
    );
    drop(common::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn execute(
    path: &Path,
    request: &[u8],
    index: u64,
    mutation: Mutation<'_>,
) -> TestResult<IdentifiedCreditReply> {
    let frame = encode_credit_request(
        &common::scope()?,
        &RequestId::new(request.to_vec())?,
        common::clock(if index == 0 { 1000 } else { 2000 })?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 512,
            configuration: None,
            mutation,
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    let mut store = common::open(path)?;
    let result = apply_credit_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        common::driver_limits(),
        CreditLimits::default(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, control } = result else {
        return Err("missing actual burst retained result".into());
    };
    assert_eq!(control.position().get(), index);
    f::reply(&outcome)
}

fn setup(path: &Path, mixed: bool) -> TestResult {
    let scope = common::scope()?;
    drop(RedbStateStore::create_credit_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &[],
            policies: &[],
        },
        f::defaults(),
        512,
        StateStoreLimits::default(),
    )?);
    assert_eq!(
        execute(path, b"empty", 0, Mutation::Get)?.decision,
        Decision::Rejected(Rejection::NotFound)
    );
    let count = if mixed { 260_u64 } else { 257 };
    let leases = (0..count).map(|shard| lease(shard, mixed)).collect();
    let owner = restore_credit_owner(
        CreditOwnerData {
            scope: b"global".to_vec(),
            limit: count * 2,
            free: 0,
            epoch: count,
            config_version: 0,
            policy_fingerprint: None,
            cleanup_head: 1,
            cleanup_tail: 0,
            leases,
        },
        CreditLimits::default(),
    )?;
    let database = redb::Builder::new().open(path)?;
    let mut tx = database.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        let bytes = encode_credit_owner(&owner, 900_000, 16 * common::MIB)?;
        table.insert(
            super::key(&scope, 12, 0, 0, &[]).as_slice(),
            bytes.as_slice(),
        )?;
        for lease in &owner.view().leases {
            if lease.reservation_page != 0 {
                put_page(&mut table, &scope, lease)?;
            }
        }
    }
    tx.commit()?;
    drop(database);
    drop(common::open(path)?);
    assert_eq!(f::family_count(path, 15)?, 0);
    Ok(())
}

fn lease(shard: u64, mixed: bool) -> CreditLeaseData {
    let paged = !mixed || shard < 258;
    let live = mixed && (shard == 0 || shard == 259);
    CreditLeaseData {
        shard_id: shard,
        epoch: shard + 1,
        expires_at_ms: if live { 5000 } else { 2000 },
        available: if paged { 1 } else { 2 },
        in_use: u64::from(paged),
        reservation_page: u16::from(paged),
        reservation_page_fill: u16::from(paged),
        ..CreditLeaseData::default()
    }
}

fn put_page(
    table: &mut redb::Table<'_, &'static [u8], &'static [u8]>,
    scope: &RequestScope,
    lease: &CreditLeaseData,
) -> TestResult {
    let mut page = b"FWGP\0\x01".to_vec();
    page.extend_from_slice(&lease.shard_id.to_be_bytes());
    page.extend_from_slice(&lease.epoch.to_be_bytes());
    page.extend_from_slice(&1_u16.to_be_bytes());
    page.extend_from_slice(&1_u32.to_be_bytes());
    page.extend_from_slice(&4_u32.to_be_bytes());
    page.extend_from_slice(b"held");
    let mut reservation = b"FWGI\0\x01".to_vec();
    reservation.extend_from_slice(&4_u32.to_be_bytes());
    reservation.extend_from_slice(b"held");
    reservation.extend_from_slice(&1_u16.to_be_bytes());
    reservation.push(0);
    table.insert(physical(scope, 14, lease).as_slice(), page.as_slice())?;
    table.insert(
        physical(scope, 13, lease).as_slice(),
        reservation.as_slice(),
    )?;
    Ok(())
}

fn physical(scope: &RequestScope, family: u8, lease: &CreditLeaseData) -> Vec<u8> {
    let mut out = b"FWPK\0\x01".to_vec();
    out.push(family);
    for part in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        b"global",
    ] {
        super::component(&mut out, part);
    }
    super::component(&mut out, &lease.shard_id.to_be_bytes());
    super::component(&mut out, &lease.epoch.to_be_bytes());
    if family == 13 {
        super::component(&mut out, b"held");
    } else {
        out.extend_from_slice(&1_u64.to_be_bytes());
    }
    out
}
