//! Physical consumed-ID custody fixtures; only the actual credit driver retires IDs.
use super::*;
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::{
    CreditConfiguration, IdentifiedCreditMutation as Mutation, IdentifiedCreditRequest,
};
use wf_ports::log::encode_credit_request;
use wf_storage::transaction::apply_credit_transaction;
use wf_types::identity::RequestId;

fn paid(path: &Path) -> TestResult {
    let scope = crate::fixtures::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let mut store = RedbStateStore::create_governed_claim_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        f::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    execute(
        &mut store,
        b"lease",
        0,
        Some(CreditConfiguration {
            limit: 3,
            config_version: Some(1),
            policy_fingerprint: None,
        }),
        Mutation::Lease {
            shard_id: 0,
            amount: 3,
            ttl_ms: 1000,
        },
    )?;
    execute(
        &mut store,
        b"spend",
        1,
        None,
        Mutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: None,
            reservation_ids: &[b"a".to_vec(), b"b".to_vec()],
        },
    )?;
    Ok(())
}

fn execute(
    store: &mut RedbStateStore,
    request: &[u8],
    index: u64,
    configuration: Option<CreditConfiguration>,
    mutation: Mutation<'_>,
) -> TestResult {
    execute_at(store, request, index, 1000 + index, configuration, mutation)
}

fn execute_at(
    store: &mut RedbStateStore,
    request: &[u8],
    index: u64,
    now: u64,
    configuration: Option<CreditConfiguration>,
    mutation: Mutation<'_>,
) -> TestResult {
    let scope = crate::fixtures::scope()?;
    let encoded = encode_credit_request(
        &scope,
        &RequestId::new(request.to_vec())?,
        crate::fixtures::clock(now)?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 4,
            configuration,
            mutation,
        },
        f::limits().record_batch.codec,
        16 * 1024 * 1024,
    )?;
    let result = apply_credit_transaction(
        store,
        &scope,
        encoded.as_bytes(),
        crate::fixtures::log(index),
        f::limits().record_batch,
        CreditLimits::default(),
    )?;
    let wf_storage::transaction::RecordBatchTransactionOutcome::Retained { outcome, .. } = result
    else {
        return Err("credit outcome missing".into());
    };
    assert_eq!(
        outcome.status(),
        wf_domain::workflow::request::RetainedStatus::Accepted
    );
    Ok(())
}

fn consumed(id: &[u8], epoch: u64) -> TestResult<Vec<u8>> {
    let scope = crate::fixtures::scope()?;
    let mut out = b"FWPK\0\x01\x18".to_vec();
    for part in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        b"global",
        &0_u64.to_be_bytes(),
        &epoch.to_be_bytes(),
        id,
    ] {
        for byte in part {
            out.push(*byte);
            if *byte == 0 {
                out.push(255);
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    Ok(out)
}

#[test]
fn claimed_ids_survive_reopen_and_exact_release_preserves_consumed_sibling() -> TestResult {
    let dir = crate::fixtures::directory("claimed-credit-release")?;
    let path = dir.join("state");
    paid(&path)?;
    let a = consumed(b"a", 1)?;
    let b = consumed(b"b", 1)?;
    edit(
        &path,
        &[
            (a.clone(), b"FWGU\0\x01".to_vec()),
            (b.clone(), b"FWGU\0\x01".to_vec()),
        ],
        &[],
    )?;
    let mut store = open(&path)?;
    execute(
        &mut store,
        b"release-a",
        2,
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &[b"a".to_vec()],
        },
    )?;
    drop(store);
    drop(open(&path)?);
    let rows = crate::fixtures::raw_rows(&path)?;
    assert!(!rows.contains_key(&a));
    assert_eq!(
        rows.get(&b).map(Vec::as_slice),
        Some(b"FWGU\0\x01".as_slice())
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn consumed_id_requires_exact_active_reservation_and_complete_marker() -> TestResult {
    for (name, id, epoch, value) in [
        (
            "missing",
            b"missing".as_slice(),
            1,
            b"FWGU\0\x01".as_slice(),
        ),
        ("epoch", b"a", 2, b"FWGU\0\x01"),
        ("tail", b"a", 1, b"FWGU\0\x01\0"),
    ] {
        let dir = crate::fixtures::directory(name)?;
        let path = dir.join("state");
        paid(&path)?;
        edit(&path, &[(consumed(id, epoch)?, value.to_vec())], &[])?;
        let before = crate::fixtures::raw_rows(&path)?;
        assert_eq!(open(&path).err(), Some(E::Corrupt));
        assert_eq!(crate::fixtures::raw_rows(&path)?, before);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn expiry_retains_consumed_ids_until_actual_reservation_page_cleanup() -> TestResult {
    let dir = crate::fixtures::directory("claimed-credit-cleanup")?;
    let path = dir.join("state");
    paid(&path)?;
    let a = consumed(b"a", 1)?;
    edit(&path, &[(a.clone(), b"FWGU\0\x01".to_vec())], &[])?;
    let mut store = open(&path)?;
    execute_at(&mut store, b"expire", 2, 2000, None, Mutation::Get)?;
    drop(store);
    drop(open(&path)?);
    assert!(crate::fixtures::raw_rows(&path)?.contains_key(&a));
    let mut store = open(&path)?;
    execute_at(&mut store, b"cleanup", 3, 2001, None, Mutation::Cleanup)?;
    drop(store);
    drop(open(&path)?);
    assert!(!crate::fixtures::raw_rows(&path)?.contains_key(&a));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn claimed_marker_survives_received_snapshot_and_retires_in_restored_writer() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let dir = crate::fixtures::directory("claimed-credit-snapshot")?;
    let path = dir.join("state");
    let target = dir.join("received");
    paid(&path)?;
    let a = consumed(b"a", 1)?;
    edit(&path, &[(a.clone(), b"FWGU\0\x01".to_vec())], &[])?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let mut wire = Vec::new();
    store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(store);
    let stage = RedbSnapshotStage::receive(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = stage.into_store()?;
    execute(
        &mut restored,
        b"release",
        2,
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &[b"a".to_vec()],
        },
    )?;
    drop(restored);
    drop(open(&target)?);
    assert!(!crate::fixtures::raw_rows(&target)?.contains_key(&a));
    assert!(crate::fixtures::raw_rows(&path)?.contains_key(&a));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn older_capability_refuses_consumed_marker_without_changing_rows() -> TestResult {
    use redb::ReadableTable;
    use sha2::{Digest, Sha256};
    let dir = crate::fixtures::directory("claimed-credit-downgrade")?;
    let path = dir.join("state");
    paid(&path)?;
    let a = consumed(b"a", 1)?;
    edit(&path, &[(a.clone(), b"FWGU\0\x01".to_vec())], &[])?;
    let db = redb::Database::open(&path)?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_meta_v1",
        ))?;
        let mut bytes = table
            .get(b"manifest".as_slice())?
            .ok_or("manifest")?
            .value()
            .to_vec();
        let at = bytes.len() - 36;
        assert_eq!(&bytes[at..at + 4], b"GC\0\x05");
        bytes[at + 3] = 4;
        let end = bytes.len() - 32;
        let checksum = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&checksum);
        table.insert(b"manifest".as_slice(), bytes.as_slice())?;
    }
    tx.commit()?;
    drop(db);
    let before = crate::fixtures::raw_rows(&path)?;
    assert_eq!(open(&path).err(), Some(E::Corrupt));
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    edit(&path, &[], &[a])?;
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn active_binding_requires_consumed_marker_and_can_outlive_exact_release() -> TestResult {
    use wf_ports::log::{ReleaseBindingRef, encode_release_binding};
    use wf_ports::state::GroupApplyStore;
    use wf_types::identity::RecordId;
    let dir = crate::fixtures::directory("claimed-credit-binding")?;
    let path = dir.join("state");
    paid(&path)?;
    let scope = crate::fixtures::scope()?;
    let mut store = open(&path)?;
    let create = f::batch(
        5,
        b"create",
        1002,
        PreparedOperation::Batch(vec![f::item(
            b"i",
            1002,
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: None,
            }),
        )?]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    f::apply_value(&mut store, &scope, &create, 2)?;
    drop(store);
    let binding = encode_release_binding(
        ReleaseBindingRef {
            epoch: 1,
            intent: ReleaseIntentRef {
                flow_id: b"i",
                partition: Some(b"p"),
                scope: b"global",
                shard_id: 0,
                reservation_id: b"a",
                enforcement: LimitEnforcement::StrictGlobal,
                created_at: TimestampMillis::new(1002),
            },
        },
        65536,
    )?;
    edit(&path, &[(key(20, None, b"i")?, binding)], &[])?;
    let before = crate::fixtures::raw_rows(&path)?;
    assert_eq!(open(&path).err(), Some(E::Corrupt));
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    edit(&path, &[(consumed(b"a", 1)?, b"FWGU\0\x01".to_vec())], &[])?;
    let mut store = open(&path)?;
    execute(
        &mut store,
        b"release-a",
        3,
        None,
        Mutation::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &[b"a".to_vec()],
        },
    )?;
    drop(store);
    let mut store = open(&path)?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        f::limits().record_batch.codec.admission(),
        1000,
        65536,
        65536,
    )?;
    let (epoch, observed) = tx
        .workflow_governance_binding(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("binding")?;
    assert_eq!(epoch, 1);
    assert_eq!(observed.reservation_id, b"a");
    drop(tx);
    drop(store);
    assert!(!crate::fixtures::raw_rows(&path)?.contains_key(&consumed(b"a", 1)?));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
