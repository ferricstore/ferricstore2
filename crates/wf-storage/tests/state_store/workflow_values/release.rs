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
    drop(store);
    assert_eq!(super::claim::fixtures::cohort(&path)?, before);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
