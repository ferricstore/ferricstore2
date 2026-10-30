use super::{fixtures as f, *};
use redb::{Durability, TableDefinition};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::{snapshot::RedbSnapshotStage, state::RedbStateStore};

#[test]
fn credit_store_reopen_preserves_detached_ids_and_original_spend_response() -> TestResult {
    let dir = common::directory("credit-reopen")?;
    let path = dir.join("state");
    f::seed(&path)?;
    let owner = f::owner(&path)?;
    let before = f::cohort(&path)?;
    drop(common::open(&path)?);
    assert_eq!(f::cohort(&path)?, before);
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    let original = f::frame(
        b"spend",
        1001,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: None,
            reservation_ids: &ids,
        },
    )?;
    let retained = f::retained(&path, &original, 2)?;
    assert_eq!(retained.original_position(), common::log(1));
    assert_eq!(retained.original_time().get(), 1001);
    assert_eq!(f::reply(&retained)?.reservation_ids, ids);
    assert_eq!(f::owner(&path)?, owner);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_snapshot_restore_preserves_owner_pages_tombstones_and_retained_outcomes()
-> TestResult {
    let dir = common::directory("credit-snapshot")?;
    let path = dir.join("source");
    f::seed(&path)?;
    let a = vec![b"a".to_vec()];
    f::execute(
        &path,
        b"release-a",
        (2, 1002),
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &a,
        },
    )?;
    let owner = f::owner(&path)?;
    let rows = common::raw_rows(&path)?;
    let mut source = common::open(&path)?;
    let mut wire = Vec::new();
    let metadata = source.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(metadata.control().position().get(), 2);
    source.pin_snapshot(*metadata.id(), SnapshotLimits::default())?;
    drop(source);
    drop(common::open(&path)?);
    let scope = common::scope()?;
    let stage = RedbSnapshotStage::receive(
        &dir.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        wf_storage::state::StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target_path = dir.join("target");
    let mut target = f::create(&target_path)?;
    assert_eq!(
        target
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        metadata.id()
    );
    drop(target);
    assert_eq!(common::raw_rows(&target_path)?, rows);
    assert_eq!(f::owner(&target_path)?, owner);
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    assert_eq!(
        f::execute(
            &target_path,
            b"new-retry",
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
        Decision::Spent { shard_id: 0 }
    );
    assert_eq!(f::owner(&target_path)?, owner);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn remove_capability(path: &std::path::Path) -> TestResult {
    let mut bytes = f::cohort(path)?
        .get("wf_state_meta_v1")
        .and_then(|metadata| metadata.get(b"manifest".as_slice()))
        .ok_or("missing credit manifest")?
        .clone();
    let end = bytes.len().checked_sub(32).ok_or("short manifest")?;
    assert_eq!(&bytes[end - 10..end], &[0, 1, 0, 0, 0, 0, 0, 0, 0, 4]);
    bytes.drain(end - 10..end);
    let end = bytes.len() - 32;
    let checksum = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&checksum);
    let database = redb::Builder::new().open(path)?;
    let mut tx = database.begin_write()?;
    tx.set_durability(Durability::Immediate)?;
    tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_meta_v1"))?
        .insert(b"manifest".as_slice(), bytes.as_slice())?;
    tx.commit()?;
    Ok(())
}

#[test]
fn credit_store_capability_downgrade_refuses_rows_and_receipt_only_owners() -> TestResult {
    let dir = common::directory("credit-downgrade")?;
    for seeded in [false, true] {
        let path = dir.join(if seeded { "seeded" } else { "receipt-only" });
        if seeded {
            f::seed(&path)?;
        } else {
            drop(f::create(&path)?);
            assert_eq!(
                f::execute(&path, b"absent", (0, 1000), None, Mutation::Get)?.decision,
                Decision::Rejected(Rejection::NotFound)
            );
            assert_eq!(f::family_count(&path, 12)?, 0);
            assert_eq!(f::family_count(&path, 13)?, 0);
        }
        remove_capability(&path)?;
        let before = f::cohort(&path)?;
        let scope = common::scope()?;
        assert!(matches!(
            RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                wf_storage::state::StateStoreLimits::default()
            ),
            Err(StateTransactionError::Corrupt)
        ));
        assert_eq!(f::cohort(&path)?, before);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_empty_capable_reopen_observes_absence_without_mutating_any_table() -> TestResult {
    let dir = common::directory("credit-empty-owner")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let before = f::cohort(&path)?;
    let store = common::open(&path)?;
    assert_eq!(
        store.read_credit_owner(
            &common::scope()?,
            b"global",
            CreditLimits::default(),
            1000,
            64 * common::MIB
        )?,
        None
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn credit_store_recovery_rejects_missing_or_orphaned_detached_graph_rows() -> TestResult {
    let dir = common::directory("credit-graph-corruption")?;
    for variant in 0..6 {
        let path = dir.join(variant.to_string());
        f::seed(&path)?;
        corrupt_graph(&path, variant)?;
        let before = f::cohort(&path)?;
        let scope = common::scope()?;
        assert!(
            matches!(
                RedbStateStore::open(
                    &path,
                    scope.group().clone(),
                    scope.recovery().clone(),
                    wf_storage::state::StateStoreLimits::default()
                ),
                Err(StateTransactionError::Corrupt)
            ),
            "graph variant {variant}"
        );
        assert_eq!(
            f::cohort(&path)?,
            before,
            "recovery changed variant {variant}"
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn corrupt_graph(path: &std::path::Path, variant: u8) -> TestResult {
    let rows = common::raw_rows(path)?;
    let (id_key, id_value) = rows
        .iter()
        .find(|(key, _)| key.get(6) == Some(&13) && key.ends_with(b"a\0\0"))
        .ok_or("missing a reservation")?;
    let (page_key, page_value) = rows
        .iter()
        .find(|(key, _)| key.get(6) == Some(&14))
        .ok_or("missing reservation page")?;
    let (owner_key, _) = rows
        .iter()
        .find(|(key, _)| key.get(6) == Some(&12))
        .ok_or("missing credit owner")?;
    let database = redb::Builder::new().open(path)?;
    let mut tx = database.begin_write()?;
    tx.set_durability(Durability::Immediate)?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        match variant {
            0 => {
                table.remove(id_key.as_slice())?;
            }
            1 => {
                let (key, value) = orphan_id(id_key, id_value, 1);
                table.insert(key.as_slice(), value.as_slice())?;
            }
            2 => {
                let mut value = id_value.clone();
                let end = value.len();
                value[end - 3..end - 1].copy_from_slice(&2_u16.to_be_bytes());
                table.insert(id_key.as_slice(), value.as_slice())?;
            }
            3 => {
                table.remove(page_key.as_slice())?;
            }
            4 => {
                let (key, value) = orphan_id(id_key, id_value, 2);
                table.insert(key.as_slice(), value.as_slice())?;
                let mut key = page_key.clone();
                let end = key.len();
                key[end - 8..].copy_from_slice(&2_u64.to_be_bytes());
                let mut value = page_value[..28].to_vec();
                value[22..24].copy_from_slice(&2_u16.to_be_bytes());
                value[24..28].copy_from_slice(&1_u32.to_be_bytes());
                value.extend_from_slice(&1_u32.to_be_bytes());
                value.push(b'x');
                table.insert(key.as_slice(), value.as_slice())?;
            }
            5 => {
                let mut key = owner_key.clone();
                key[6] = 15;
                key.extend_from_slice(&1_u64.to_be_bytes());
                let mut value = b"FWGQ\0\x01".to_vec();
                for scalar in [1_u64, 0, 1] {
                    value.extend_from_slice(&scalar.to_be_bytes());
                }
                value.extend_from_slice(&1_u16.to_be_bytes());
                value.extend_from_slice(&1_u16.to_be_bytes());
                table.insert(key.as_slice(), value.as_slice())?;
            }
            _ => return Err("unknown graph corruption".into()),
        }
    }
    tx.commit()?;
    Ok(())
}

fn orphan_id(key: &[u8], value: &[u8], page: u16) -> (Vec<u8>, Vec<u8>) {
    let mut key = key.to_vec();
    let end = key.len();
    key[end - 3] = b'x';
    let mut value = value.to_vec();
    value[10] = b'x';
    let end = value.len();
    value[end - 3..end - 1].copy_from_slice(&page.to_be_bytes());
    (key, value)
}
