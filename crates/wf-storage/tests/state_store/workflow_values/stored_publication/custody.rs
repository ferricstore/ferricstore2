//! Real complete snapshot, zero-value capability and abort/corruption custody.

use std::io::Cursor;

use wf_domain::workflow::request::RetainedStatus;
use wf_ports::blob::OriginalValueRefKind::Payload;
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::transaction::{RecordBatchTransactionError, WorkflowValueTransactionError};

use super::*;

#[test]
fn stored_publication_full_snapshot_preserves_values_and_outcomes() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-snapshot")?;
    let path = directory.join("source");
    let mut store = fresh(&path)?;
    let request = create(5, b"snapshot", 100, Some(b"FSV2\x02bad-etf"))?;
    let result = apply(&mut store, &request, 0);
    assert!(
        result.is_ok(),
        "healthy stored publication: {:?}",
        result.as_ref().err()
    );
    let original = retained(result?)?;
    assert_eq!(original.status(), RetainedStatus::Accepted);
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    store.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    let meta = metadata(&path)?;
    assert_eq!(captured.row_count(), rows.len() + 3);
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        request.scope.group().clone(),
        request.scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target_path = directory.join("target");
    let mut target = fresh(&target_path)?;
    assert_eq!(
        target
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    assert_value(&target, b"snapshot", Payload, 1, b"FSV2\x02bad-etf")?;
    let mut installed = Vec::new();
    assert_eq!(
        target
            .current_snapshot(&mut installed, SnapshotLimits::default())?
            .ok_or("artifact")?
            .id(),
        captured.id()
    );
    assert_eq!(installed, wire);
    drop(target);
    assert_eq!(crate::fixtures::raw_rows(&target_path)?, rows);
    let target_meta = metadata(&target_path)?;
    for key in [b"manifest".as_slice(), b"membership", b"origin"] {
        assert_eq!(target_meta.get(key), meta.get(key));
    }
    let mut target = open(&target_path)?;
    assert_eq!(retained(apply(&mut target, &request, 1)?)?, original);
    assert_value(&target, b"snapshot", Payload, 1, b"FSV2\x02bad-etf")?;
    drop(target);
    drop(open(&target_path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stored_publication_recovery_rejects_profile_downgrade_without_alias_rows() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-zero-values")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let request = create(5, b"no-value", 100, None)?;
    let result = apply(&mut store, &request, 0);
    assert!(
        result.is_ok(),
        "healthy stored publication: {:?}",
        result.as_ref().err()
    );
    assert_eq!(retained(result?)?.intent().version(), 9);
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    assert_eq!(
        rows.keys()
            .filter(|key| matches!(key.get(6), Some(8 | 9)))
            .count(),
        0
    );
    assert_eq!(rows.keys().filter(|key| key.get(6) == Some(&2)).count(), 1);
    drop(open(&path)?);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    let mut meta = metadata(&path)?;
    let manifest = meta.get_mut(b"manifest".as_slice()).ok_or("manifest")?;
    let at =
        108 + request.scope.group().as_bytes().len() + request.scope.recovery().as_bytes().len();
    assert_eq!(&manifest[at..at + 4], 2_u32.to_be_bytes());
    manifest[at..at + 4].copy_from_slice(&1_u32.to_be_bytes());
    reseal(manifest)?;
    rewrite(&path, b"manifest", manifest, true)?;
    let refused = RedbStateStore::open(
        &path,
        request.scope.group().clone(),
        request.scope.recovery().clone(),
        StateStoreLimits::default(),
    );
    assert!(matches!(refused, Err(StateTransactionError::Corrupt)));
    assert_eq!(metadata(&path)?, meta);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn short_limits(case: u8) -> WorkflowValueTransactionLimits {
    let mut limits = f::limits();
    match case {
        0 => limits.maximum_value_rows = 1,
        1 => limits.maximum_value_bytes = 1,
        2 => limits.maximum_value_work = 1,
        _ => limits.maximum_value_temporary_bytes = 1,
    }
    limits
}

#[test]
fn stored_publication_resource_and_corruption_refusals_are_atomic() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-refusals")?;
    for case in 0..4 {
        let path = directory.join(format!("limit-{case}"));
        let mut store = fresh(&path)?;
        let request = create(5, b"baseline", 100, Some(b"FSV2\x02baseline"))?;
        let result = apply(&mut store, &request, 0);
        assert!(
            result.is_ok(),
            "healthy stored publication: {:?}",
            result.as_ref().err()
        );
        assert_eq!(retained(result?)?.status(), RetainedStatus::Accepted);
        drop(store);
        let rows = crate::fixtures::raw_rows(&path)?;
        let meta = metadata(&path)?;
        let mut store = open(&path)?;
        let next = create(5, b"next", 101, Some(b"FSV2\x02next"))?;
        let failed = apply_with(&mut store, &next, 1, short_limits(case));
        let error = failed.err().ok_or("short resource limit was accepted")?;
        assert!(matches!(
            error.downcast_ref::<WorkflowValueTransactionError>(),
            Some(WorkflowValueTransactionError::Record(
                RecordBatchTransactionError::Transaction(StateTransactionError::Limit)
            ))
        ));
        drop(store);
        assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
        assert_eq!(metadata(&path)?, meta);
        let mut store = open(&path)?;
        assert_eq!(
            retained(apply(&mut store, &next, 1)?)?.status(),
            RetainedStatus::Accepted
        );
        assert_value(&store, b"next", Payload, 1, b"FSV2\x02next")?;
        drop(store);
    }
    let path = directory.join("limit-0");
    let mut rows = crate::fixtures::raw_rows(&path)?;
    let key = rows
        .keys()
        .find(|key| key.get(6) == Some(&8))
        .ok_or("immutable row")?
        .clone();
    let bytes = rows.get_mut(&key).ok_or("immutable bytes")?;
    bytes[0] ^= 1;
    rewrite(&path, &key, bytes, false)?;
    let scope = crate::fixtures::scope()?;
    assert!(matches!(
        RedbStateStore::open(
            &path,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::Corrupt)
    ));
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
