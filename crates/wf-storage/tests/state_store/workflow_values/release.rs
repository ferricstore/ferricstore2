//! Explicit physical schema fixtures; these do not simulate governed Claim success.
use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::governance::release::{LimitEnforcement, ReleaseIntentRef, ReleaseOutboxState};
use wf_domain::workflow::{Create, Operation};
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{encode_release_intent, encode_release_meta};
use wf_ports::state::StateTransactionError as E;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_types::{identity::PartitionId, version::TimestampMillis};
mod custody;
mod mutations;

fn fixture(path: &Path) -> TestResult {
    let scope = crate::fixtures::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let mut store = RedbStateStore::create_workflow_release_database(
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
    let item = f::item(
        b"i",
        100,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(b"payload".to_vec()),
        }),
    )?;
    let prepared = f::batch(
        5,
        b"create",
        100,
        PreparedOperation::Batch(vec![item]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    f::apply_value(&mut store, &scope, &prepared, 0)?;
    Ok(())
}

fn open(path: &Path) -> Result<RedbStateStore, E> {
    let scope = crate::fixtures::scope().map_err(|_| E::InvalidChanges)?;
    RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )
}

fn key(family: u8, sequence: Option<u64>, id: &[u8]) -> TestResult<Vec<u8>> {
    let scope = crate::fixtures::scope()?;
    let mut key = b"FWPK\0\x01".to_vec();
    key.push(family);
    let mut parts = vec![
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
    ];
    if family == 20 {
        parts.extend_from_slice(&[b"p", id]);
    }
    for part in parts {
        for byte in part {
            key.push(*byte);
            if *byte == 0 {
                key.push(255);
            }
        }
        key.extend_from_slice(&[0, 0]);
    }
    if let Some(n) = sequence {
        key.extend_from_slice(&n.to_be_bytes());
    }
    Ok(key)
}

fn intent(id: &[u8], reservation: &[u8]) -> TestResult<Vec<u8>> {
    Ok(encode_release_intent(
        ReleaseIntentRef {
            flow_id: id,
            partition: Some(b"p"),
            scope: b"limit\0scope",
            shard_id: 3,
            reservation_id: reservation,
            enforcement: LimitEnforcement::StrictGlobal,
            created_at: TimestampMillis::new(100),
        },
        65536,
    )?)
}

fn binding(id: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWGB\0\x01".to_vec();
    bytes.extend_from_slice(&7_u64.to_be_bytes());
    bytes.extend_from_slice(&intent(id, b"paid\0id")?);
    Ok(bytes)
}

fn edit(path: &Path, inserts: &[(Vec<u8>, Vec<u8>)], deletes: &[Vec<u8>]) -> TestResult {
    let db = redb::Database::open(path)?;
    let tx = db.begin_write()?;
    {
        let mut rows = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_rows_v1",
        ))?;
        for (key, value) in inserts {
            rows.insert(key.as_slice(), value.as_slice())?;
        }
        for key in deletes {
            rows.remove(key.as_slice())?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn queue(path: &Path) -> TestResult {
    edit(
        path,
        &[
            (key(20, None, b"i")?, binding(b"i")?),
            (
                key(21, None, b"")?,
                encode_release_meta(ReleaseOutboxState { head: 1, tail: 2 }, 22)?,
            ),
            (key(22, Some(1), b"")?, intent(b"i", b"paid\0id")?),
            (
                key(22, Some(2), b"")?,
                intent(b"already-retained-away", b"sibling")?,
            ),
            (key(23, Some(2), b"")?, b"FWRC\0\x01".to_vec()),
        ],
        &[],
    )
}

#[test]
fn workflow_release_recovery_accepts_exact_binding_and_contiguous_sparse_queue() -> TestResult {
    let dir = crate::fixtures::directory("release-data-valid")?;
    let path = dir.join("state");
    fixture(&path)?;
    queue(&path)?;
    let before = crate::fixtures::raw_rows(&path)?;
    let store = open(&path);
    assert!(
        store.is_ok(),
        "valid release data refused: {:?}",
        store.as_ref().err()
    );
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_recovery_accepts_exhausted_empty_queue_and_binding_without_meta() -> TestResult
{
    for with_meta in [false, true] {
        let dir = crate::fixtures::directory("release-data-empty")?;
        let path = dir.join("state");
        fixture(&path)?;
        edit(&path, &[(key(20, None, b"i")?, binding(b"i")?)], &[])?;
        if with_meta {
            let max = wf_domain::workflow::MAX_EXACT_MILLIS;
            edit(
                &path,
                &[(
                    key(21, None, b"")?,
                    encode_release_meta(
                        ReleaseOutboxState {
                            head: max + 1,
                            tail: max,
                        },
                        22,
                    )?,
                )],
                &[],
            )?;
        }
        let store = open(&path);
        assert!(
            store.is_ok(),
            "valid empty queue refused: {:?}",
            store.as_ref().err()
        );
        drop(store);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn workflow_release_recovery_fences_missing_and_orphan_rows_without_mutating_cohort() -> TestResult
{
    for case in 0..7 {
        let dir = crate::fixtures::directory("release-data-corrupt")?;
        let path = dir.join("state");
        fixture(&path)?;
        queue(&path)?;
        match case {
            0 => edit(&path, &[], &[key(22, Some(1), b"")?])?,
            1 => edit(&path, &[], &[key(21, None, b"")?])?,
            2 => edit(
                &path,
                &[(key(22, Some(3), b"")?, intent(b"i", b"orphan")?)],
                &[],
            )?,
            3 => edit(
                &path,
                &[(key(23, Some(3), b"")?, b"FWRC\0\x01".to_vec())],
                &[],
            )?,
            4 => edit(
                &path,
                &[(key(20, None, b"missing")?, binding(b"missing")?)],
                &[],
            )?,
            5 => edit(
                &path,
                &[(key(20, None, b"i")?, binding(b"wrong-record")?)],
                &[],
            )?,
            _ => edit(
                &path,
                &[(key(23, Some(2), b"")?, b"FWRC\0\x01trailing".to_vec())],
                &[],
            )?,
        }
        let before = crate::fixtures::raw_rows(&path)?;
        assert_eq!(open(&path).err(), Some(E::Corrupt), "case {case}");
        assert_eq!(crate::fixtures::raw_rows(&path)?, before, "case {case}");
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn workflow_release_legacy_mutations_cannot_write_bound_records_without_releases() -> TestResult {
    use wf_domain::workflow::Cancel;
    use wf_ports::log::encode_workflow_value_batch;
    use wf_storage::transaction::apply_workflow_value_batch_transaction;
    for profile in [5, 6] {
        let dir = crate::fixtures::directory("release-legacy-mutation")?;
        let path = dir.join("state");
        fixture(&path)?;
        edit(&path, &[(key(20, None, b"i")?, binding(b"i")?)], &[])?;
        let before = super::claim::fixtures::cohort(&path)?;
        let mut store = open(&path)?;
        let item = f::item(
            b"i",
            200,
            Operation::Cancel(Cancel {
                fence: wf_types::version::Fence::new(0),
                lease_token: None,
                expected_state: Some(b"q".to_vec()),
                expected_version: Some(1),
                error: None,
            }),
        )?;
        let batch = f::batch(
            profile,
            b"cancel",
            200,
            PreparedOperation::Batch(vec![item]),
            vec![f::input(0, 0)],
            vec![f::witness(b"r", None)?],
        )?;
        let frame = encode_workflow_value_batch(&batch, f::limits().record_batch.codec)?;
        let applied = apply_workflow_value_batch_transaction(
            &mut store,
            &crate::fixtures::scope()?,
            frame.as_bytes(),
            crate::fixtures::log(1),
            f::limits(),
        );
        assert!(
            applied.is_err(),
            "legacy mode {profile} silently modified a bound record"
        );
        assert!(matches!(
            applied,
            Err(
                wf_storage::transaction::WorkflowValueTransactionError::Record(
                    wf_storage::transaction::RecordBatchTransactionError::Transaction(
                        E::UnsupportedPolicyProfile
                    )
                )
            )
        ));
        drop(store);
        assert_eq!(super::claim::fixtures::cohort(&path)?, before);
        drop(open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn workflow_release_legacy_claim_cannot_overwrite_a_bound_record() -> TestResult {
    let dir = crate::fixtures::directory("release-legacy-claim")?;
    let path = dir.join("state");
    fixture(&path)?;
    edit(&path, &[(key(20, None, b"i")?, binding(b"i")?)], &[])?;
    let before = super::claim::fixtures::cohort(&path)?;
    let mut store = open(&path)?;
    let frame = super::claim::fixtures::phase(b"claim", 200, b"q", b"worker", 1)?;
    let applied = wf_storage::transaction::apply_workflow_value_claim_transaction(
        &mut store,
        &crate::fixtures::scope()?,
        frame.as_bytes(),
        crate::fixtures::log(1),
        f::limits(),
        wf_domain::claim::apply::ClaimWorkLimits::default(),
    );
    assert!(
        applied.is_err(),
        "legacy Claim silently replaced a bound record"
    );
    assert!(matches!(
        applied,
        Err(wf_storage::transaction::ClaimTransactionError::Transaction(
            E::UnsupportedPolicyProfile
        ))
    ));
    drop(store);
    assert_eq!(super::claim::fixtures::cohort(&path)?, before);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_page_preserves_original_bytes_sparse_completion_and_scope() -> TestResult {
    use wf_ports::state::GroupApplyStore;
    use wf_types::identity::{RecordId, TenantId};
    let dir = crate::fixtures::directory("release-read-page")?;
    let path = dir.join("state");
    fixture(&path)?;
    queue(&path)?;
    let mut store = open(&path)?;
    let scope = crate::fixtures::scope()?;
    let first = store.read_workflow_release_outbox(&scope, None, 1, 1000, 65536)?;
    assert_eq!((first.meta.head, first.meta.tail, first.more), (1, 2, true));
    assert_eq!(first.entries.len(), 1);
    assert_eq!(
        (first.entries[0].sequence, first.entries[0].completed),
        (1, false)
    );
    assert_eq!(first.entries[0].encoded(), intent(b"i", b"paid\0id")?);
    assert_eq!(first.entries[0].intent()?.reservation_id, b"paid\0id");
    let next = store.read_workflow_release_outbox(&scope, Some(1), 256, 1000, 65536)?;
    assert_eq!(next.entries.len(), 1);
    assert_eq!(
        (
            next.entries[0].sequence,
            next.entries[0].completed,
            next.more
        ),
        (2, true, false)
    );
    assert_eq!(next.entries[0].intent()?.flow_id, b"already-retained-away");
    let end = store.read_workflow_release_outbox(&scope, Some(2), 1, 1000, 65536)?;
    assert_eq!(end.entries.len(), 0);
    assert!(!end.more);
    for limit in [0, 257] {
        assert_eq!(
            store
                .read_workflow_release_outbox(&scope, None, limit, 1000, 65536)
                .err(),
            Some(E::InvalidChanges)
        );
    }
    assert_eq!(
        store
            .read_workflow_release_outbox(&scope, None, 2, 1, 65536)
            .err(),
        Some(E::Limit)
    );
    assert_eq!(
        store
            .read_workflow_release_outbox(&scope, None, 2, 1000, 1)
            .err(),
        Some(E::Limit)
    );
    let other = wf_domain::workflow::request::RequestScope::new(
        TenantId::new(b"other".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    let absent = store.read_workflow_release_outbox(&other, None, 1, 1000, 65536)?;
    assert_eq!(
        (absent.meta.head, absent.meta.tail, absent.more),
        (1, 0, false)
    );
    assert_eq!(absent.entries.len(), 0);
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        f::limits().record_batch.codec.admission(),
        1000,
        65536,
        65536,
    )?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let (epoch, reservation) = tx
        .workflow_governance_binding(&scope, &partition, &id)?
        .ok_or("missing binding")?;
    assert_eq!(epoch, 7);
    assert_eq!(reservation.scope, b"limit\0scope");
    assert_eq!(reservation.reservation_id, b"paid\0id");
    assert_eq!(reservation.shard_id, 3);
    assert!(reservation.enforcement == LimitEnforcement::StrictGlobal);
    assert!(
        tx.workflow_governance_binding(&other, &partition, &id)?
            .is_none()
    );
    drop(tx);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_snapshot_roundtrip_carries_actual_rows_and_original_owners() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let dir = crate::fixtures::directory("release-data-snapshot")?;
    let source = dir.join("source");
    let target = dir.join("target");
    fixture(&source)?;
    queue(&source)?;
    let mut store = open(&source)?;
    let mut wire = Vec::new();
    let original = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(store);
    let rows = crate::fixtures::raw_rows(&source)?;
    let scope = crate::fixtures::scope()?;
    let stage = RedbSnapshotStage::receive(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = stage.into_store()?;
    assert_eq!(restored.governance_catalog_profile(), 4);
    let page = restored.read_workflow_release_outbox(&scope, None, 256, 1000, 65536)?;
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries[0].encoded(), intent(b"i", b"paid\0id")?);
    assert!(page.entries[1].completed);
    let mut reproduced = Vec::new();
    assert_eq!(
        restored
            .current_snapshot(&mut reproduced, SnapshotLimits::default())?
            .ok_or("missing snapshot")?
            .id(),
        original.id()
    );
    assert_eq!(reproduced, wire);
    drop(restored);
    assert_eq!(crate::fixtures::raw_rows(&target)?, rows);
    drop(open(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn workflow_release_binding_fences_generation_partition_shard_and_time_corruption() -> TestResult {
    for case in 0..6 {
        let dir = crate::fixtures::directory("release-binding-corrupt")?;
        let path = dir.join("state");
        fixture(&path)?;
        let mut encoded = binding(b"i")?;
        match case {
            0 => encoded[6..14].copy_from_slice(&0_u64.to_be_bytes()),
            1 => encoded[6..14]
                .copy_from_slice(&(wf_domain::workflow::MAX_EXACT_MILLIS + 1).to_be_bytes()),
            2 => encoded.push(0),
            _ => {
                let original = wf_ports::log::inspect_release_binding(&encoded, encoded.len())?;
                let intent = ReleaseIntentRef {
                    partition: if case == 3 {
                        None
                    } else {
                        original.intent.partition
                    },
                    shard_id: if case == 4 { 4 } else { 3 },
                    created_at: TimestampMillis::new(if case == 5 { 101 } else { 100 }),
                    ..original.intent
                };
                encoded = wf_ports::log::encode_release_binding(
                    wf_ports::log::ReleaseBindingRef { epoch: 7, intent },
                    65536,
                )?;
            }
        }
        edit(&path, &[(key(20, None, b"i")?, encoded)], &[])?;
        let before = super::claim::fixtures::cohort(&path)?;
        assert_eq!(open(&path).err(), Some(E::Corrupt), "case {case}");
        assert_eq!(super::claim::fixtures::cohort(&path)?, before);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
