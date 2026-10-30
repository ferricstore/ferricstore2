//! Complete canonical donors exercise business ceilings without forged read gaps.

use super::{fixtures as f, *};
use redb::TableDefinition;
use std::{ops::RangeInclusive, path::Path};
use wf_domain::governance::credit::restore_credit_owner;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{MAX_EXACT_MILLIS as MAX, request::RetainedStatus};
use wf_ports::log::encode_credit_owner;

#[test]
fn credit_store_full_history_rejects_without_any_detached_or_owner_prefix() -> TestResult {
    let dir = common::directory("credit-full-history")?;
    let path = dir.join("state");
    let mut owner = seed(&path)?.into_data();
    owner.leases[0].available = 1;
    owner.leases[0].in_use = 1;
    owner.leases[0].reservation_page = 256;
    owner.leases[0].reservation_page_fill = 256;
    graph(
        &path,
        &restore_credit_owner(owner, CreditLimits::default())?,
        1..=1,
        256,
        256,
    )?;
    drop(common::open(&path)?);
    assert_eq!(f::family_count(&path, 13)?, 65_536);
    assert_eq!(f::family_count(&path, 14)?, 256);
    let before = business(&path)?;
    let ids = vec![b"overflow".to_vec()];
    let frame = f::frame(
        b"overflow",
        1001,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 1,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    let outcome = f::retained(&path, &frame, 1)?;
    assert_eq!(
        f::reply(&outcome)?.decision,
        Decision::Rejected(Rejection::ReservationHistoryFull)
    );
    assert_eq!(
        outcome.status(),
        RetainedStatus::GovernanceRejected(Rejection::ReservationHistoryFull)
    );
    assert_eq!(business(&path)?, before);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_generation_exhaustion_aborts_released_ids_and_owner_together() -> TestResult {
    let dir = common::directory("credit-max-generation")?;
    let path = dir.join("state");
    let mut owner = seed(&path)?.into_data();
    owner.epoch = MAX;
    owner.leases[0].epoch = MAX;
    owner.leases[0].available = 1;
    owner.leases[0].in_use = 1;
    owner.leases[0].reservation_page = 1;
    owner.leases[0].reservation_page_fill = 1;
    graph(
        &path,
        &restore_credit_owner(owner, CreditLimits::default())?,
        MAX..=MAX,
        1,
        1,
    )?;
    drop(common::open(&path)?);
    let before = business(&path)?;
    let ids = vec![0_u32.to_be_bytes().to_vec()];
    let frame = f::frame(
        b"max-release",
        1001,
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &ids,
        },
    )?;
    let outcome = f::retained(&path, &frame, 1)?;
    let reason = Rejection::Credit(CreditRejection::GenerationExhausted);
    assert_eq!(f::reply(&outcome)?.decision, Decision::Rejected(reason));
    assert_eq!(outcome.status(), RetainedStatus::GovernanceRejected(reason));
    assert_eq!(business(&path)?, before);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_full_cleanup_backlog_refuses_expiry_without_publishing_a_prefix() -> TestResult {
    let dir = common::directory("credit-full-cleanup-queue")?;
    let path = dir.join("state");
    let mut owner = seed(&path)?.into_data();
    owner.epoch = 257;
    owner.cleanup_tail = 256;
    owner.leases[0].epoch = 257;
    owner.leases[0].available = 1;
    owner.leases[0].in_use = 1;
    owner.leases[0].reservation_page = 1;
    owner.leases[0].reservation_page_fill = 1;
    graph(
        &path,
        &restore_credit_owner(owner, CreditLimits::default())?,
        1..=257,
        1,
        1,
    )?;
    drop(common::open(&path)?);
    assert_eq!(f::family_count(&path, 15)?, 256);
    let before = business(&path)?;
    let frame = f::frame(b"expire-backlog", 2000, None, Mutation::Get)?;
    let outcome = f::retained(&path, &frame, 1)?;
    assert_eq!(
        f::reply(&outcome)?.decision,
        Decision::Rejected(Rejection::CleanupBacklogFull)
    );
    assert_eq!(business(&path)?, before);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn seed(path: &Path) -> TestResult<CreditOwner> {
    drop(f::create(path)?);
    f::execute(
        path,
        b"boundary-lease",
        (0, 1000),
        Some(CreditConfiguration {
            limit: 2,
            config_version: Some(0),
            policy_fingerprint: None,
        }),
        Mutation::Lease {
            shard_id: 0,
            amount: 2,
            ttl_ms: 1000,
        },
    )?;
    f::owner(path)
}

fn business(path: &Path) -> TestResult<std::collections::BTreeMap<Vec<u8>, Vec<u8>>> {
    Ok(common::raw_rows(path)?
        .into_iter()
        .filter(|(key, _)| matches!(key.get(6), Some(12..=15)))
        .collect())
}

fn graph(
    path: &Path,
    owner: &CreditOwner,
    epochs: RangeInclusive<u64>,
    pages: u16,
    fill: u16,
) -> TestResult {
    let scope = common::scope()?;
    let database = redb::Builder::new().open(path)?;
    let mut tx = database.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        let bytes = encode_credit_owner(owner, 900_000, 16 * common::MIB)?;
        table.insert(key(&scope, 12, 0, 0, &[]).as_slice(), bytes.as_slice())?;
        let current = owner.lease(0).ok_or("missing boundary lease")?.epoch;
        for epoch in epochs {
            for page in 1..=pages {
                page_rows(&mut table, &scope, epoch, current, page, fill)?;
            }
            if epoch < current {
                table.insert(
                    key(&scope, 15, epoch, 0, &[]).as_slice(),
                    task_bytes(epoch, pages).as_slice(),
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

fn page_rows(
    table: &mut redb::Table<'_, &'static [u8], &'static [u8]>,
    scope: &RequestScope,
    epoch: u64,
    current: u64,
    page: u16,
    fill: u16,
) -> TestResult {
    let mut encoded = b"FWGP\0\x01".to_vec();
    encoded.extend_from_slice(&0_u64.to_be_bytes());
    encoded.extend_from_slice(&epoch.to_be_bytes());
    encoded.extend_from_slice(&page.to_be_bytes());
    encoded.extend_from_slice(&u32::from(fill).to_be_bytes());
    for offset in 0..u32::from(fill) {
        let ordinal = (u32::from(page) - 1) * u32::from(fill) + offset;
        let id = ordinal.to_be_bytes();
        encoded.extend_from_slice(&4_u32.to_be_bytes());
        encoded.extend_from_slice(&id);
        let mut reservation = b"FWGI\0\x01".to_vec();
        reservation.extend_from_slice(&4_u32.to_be_bytes());
        reservation.extend_from_slice(&id);
        reservation.extend_from_slice(&page.to_be_bytes());
        reservation.push(u8::from(!(epoch == current && ordinal == 0)));
        table.insert(
            key(scope, 13, epoch, page, &id).as_slice(),
            reservation.as_slice(),
        )?;
    }
    table.insert(
        key(scope, 14, epoch, page, &[]).as_slice(),
        encoded.as_slice(),
    )?;
    Ok(())
}

fn task_bytes(epoch: u64, pages: u16) -> Vec<u8> {
    let mut task = b"FWGQ\0\x01".to_vec();
    for scalar in [epoch, 0, epoch] {
        task.extend_from_slice(&scalar.to_be_bytes());
    }
    task.extend_from_slice(&1_u16.to_be_bytes());
    task.extend_from_slice(&pages.to_be_bytes());
    task
}

fn key(scope: &RequestScope, family: u8, epoch: u64, page: u16, id: &[u8]) -> Vec<u8> {
    let mut out = b"FWPK\0\x01".to_vec();
    out.push(family);
    for part in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        b"global",
    ] {
        component(&mut out, part);
    }
    if matches!(family, 13 | 14) {
        component(&mut out, &0_u64.to_be_bytes());
        component(&mut out, &epoch.to_be_bytes());
    }
    match family {
        13 => component(&mut out, id),
        14 => out.extend_from_slice(&u64::from(page).to_be_bytes()),
        15 => out.extend_from_slice(&epoch.to_be_bytes()),
        _ => {}
    }
    out
}

fn component(out: &mut Vec<u8>, part: &[u8]) {
    for byte in part {
        out.push(*byte);
        if *byte == 0 {
            out.push(255);
        }
    }
    out.extend_from_slice(&[0, 0]);
}
