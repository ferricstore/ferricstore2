//! Actual transaction uncertainty, snapshot receive and receipt-only downgrade.
use super::*;
use crate::workflow_values::claim::fixtures as c;

fn seeded(path: &Path) -> TestResult {
    fixture(path)?;
    queue(path)
}

#[test]
fn release_maintenance_uncertain_ack_preserves_complete_cohort_and_fences() -> TestResult {
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };
    use wf_ports::state::GroupApplyStore;
    use wf_storage::transaction::RecordBatchTransactionError as B;
    let dir = crate::fixtures::directory("release-maintenance-uncertain")?;
    let path = dir.join("state");
    let reference = dir.join("reference");
    seeded(&path)?;
    seeded(&reference)?;
    let old = c::cohort(&path)?;
    let command = C::Acknowledge {
        expected_head: 1,
        up_to: 1,
    };
    let mut expected = open(&reference)?;
    apply(&mut expected, b"ack", &command, 1)?;
    drop(expected);
    let complete = c::cohort(&reference)?;
    let flag = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&flag))?;
    let scope = crate::fixtures::scope()?;
    let encoded = encode_release_maintenance_request(
        &scope,
        &RequestId::new(b"ack".to_vec())?,
        crate::fixtures::clock(101)?,
        &command,
        f::limits().record_batch.codec,
        f::limits().record_batch.workspace_bytes,
    )?;
    flag.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_release_maintenance_transaction(
            &mut store,
            &scope,
            encoded.as_bytes(),
            crate::fixtures::log(1),
            f::limits().record_batch
        )
        .err(),
        Some(B::Transaction(E::Uncertain))
    );
    assert_eq!(flag.load(Ordering::SeqCst), 0);
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            f::limits().record_batch.codec.admission(),
            1000,
            65536,
            65536
        ),
        Err(E::Unavailable)
    ));
    drop(store);
    let actual = c::cohort(&path)?;
    assert!(actual == old || actual == complete);
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_maintenance_snapshot_replays_original_reply_and_preserves_sparse_tail() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let dir = crate::fixtures::directory("release-maintenance-snapshot")?;
    let path = dir.join("state");
    let target = dir.join("received");
    seeded(&path)?;
    let mut store = open(&path)?;
    let command = C::MarkCompleted { sequences: vec![2] };
    let original = apply(&mut store, b"mark", &command, 1)?;
    apply(
        &mut store,
        b"ack",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
        2,
    )?;
    let mut wire = Vec::new();
    store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(store);
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
    assert_eq!(apply(&mut restored, b"mark", &command, 3)?, original);
    let page = restored.read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?;
    assert_eq!(page.meta.head, 2);
    assert_eq!(page.entries.len(), 1);
    assert!(page.entries[0].completed);
    drop(restored);
    drop(open(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_maintenance_receipt_requires_gc4_when_no_queue_rows_exist() -> TestResult {
    use redb::ReadableTable;
    use sha2::{Digest, Sha256};
    let dir = crate::fixtures::directory("release-maintenance-receipt")?;
    let path = dir.join("state");
    fixture(&path)?;
    let mut store = open(&path)?;
    let original = apply(
        &mut store,
        b"empty",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
        1,
    )?;
    assert!(matches!(reply(&original)?, R::Rejected { .. }));
    assert!(wf_storage::state::value::encode_outcome(&original, 65536).is_err());
    drop(store);
    assert_eq!(
        crate::fixtures::raw_rows(&path)?
            .keys()
            .filter(|k| matches!(k.get(6), Some(20..=24)))
            .count(),
        0
    );
    let db = redb::Database::open(&path)?;
    let tx = db.begin_write()?;
    {
        let mut meta = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_meta_v1",
        ))?;
        let mut bytes = meta
            .get(b"manifest".as_slice())?
            .ok_or("manifest")?
            .value()
            .to_vec();
        let at = bytes.len() - 36;
        assert_eq!(&bytes[at..at + 4], b"GC\0\x04");
        bytes[at + 3] = 3;
        let end = bytes.len() - 32;
        let hash = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&hash);
        meta.insert(b"manifest".as_slice(), bytes.as_slice())?;
    }
    tx.commit()?;
    drop(db);
    let before = c::cohort(&path)?;
    assert_eq!(open(&path).err(), Some(E::Corrupt));
    assert_eq!(c::cohort(&path)?, before);
    let scope = crate::fixtures::scope()?;
    let receipt = wf_storage::state::key::encode_outcome_key(
        &scope,
        &RequestId::new(b"empty".to_vec())?,
        65535,
    )?;
    edit(&path, &[], &[receipt])?;
    drop(open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_maintenance_operational_refusal_keeps_all_pending_rows_and_control() -> TestResult {
    use wf_storage::transaction::RecordBatchTransactionError as B;
    let dir = crate::fixtures::directory("release-maintenance-resource")?;
    let path = dir.join("state");
    seeded(&path)?;
    let before = c::cohort(&path)?;
    let scope = crate::fixtures::scope()?;
    let encoded = encode_release_maintenance_request(
        &scope,
        &RequestId::new(b"ack".to_vec())?,
        crate::fixtures::clock(101)?,
        &C::Acknowledge {
            expected_head: 1,
            up_to: 2,
        },
        f::limits().record_batch.codec,
        f::limits().record_batch.workspace_bytes,
    )?;
    for steps in [1, 20] {
        let mut store = open(&path)?;
        let mut limits = f::limits().record_batch;
        limits.maximum_read_steps = steps;
        assert_eq!(
            apply_release_maintenance_transaction(
                &mut store,
                &scope,
                encoded.as_bytes(),
                crate::fixtures::log(1),
                limits
            )
            .err(),
            Some(B::Transaction(E::Limit))
        );
        drop(store);
        assert_eq!(c::cohort(&path)?, before);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
