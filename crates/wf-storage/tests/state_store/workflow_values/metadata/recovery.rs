use super::{TestResult, f, fixtures::*};
use wf_types::identity::{PartitionId, RecordId};

#[test]
fn actual_reopen_preserves_metadata_and_original_native_rows() -> TestResult {
    let directory = crate::fixtures::directory("metadata-native-reopen")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let data = metadata(7);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    drop(store);
    let before = crate::fixtures::raw_rows(&path)?;
    let reopened = crate::fixtures::open(&path)?;
    assert_eq!(
        reopened.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        )?,
        Some(data)
    );
    drop(reopened);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn reopened_authority_rejects_resealed_wrong_core_digest_and_scalar_only_binding() -> TestResult {
    use wf_ports::state::StateTransactionError;
    let directory = crate::fixtures::directory("metadata-native-corruption")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let data = metadata(7);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    drop(store);
    let original = crate::fixtures::raw_rows(&path)?;
    let manifest = f::raw_manifest(&path)?;
    for family in [10, 11] {
        for scalar_only in [false, true] {
            let mut rows = original.clone();
            let (_, bytes) = rows
                .iter_mut()
                .find(|(key, _)| key.get(6) == Some(&family))
                .ok_or("binding")?;
            assert!(bytes.starts_with(b"FWMB\0\x01"));
            let base_len = usize::try_from(u32::from_be_bytes(bytes[6..10].try_into()?))?;
            if scalar_only {
                *bytes = bytes[10..10 + base_len].to_vec();
            } else {
                bytes[10 + base_len + 8] ^= 1;
                reseal(bytes);
            }
            replace_rows(&path, &rows)?;
            assert!(matches!(crate::fixtures::open(&path), Err(error)
                if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt)));
            assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
            assert_eq!(f::raw_manifest(&path)?, manifest);
        }
    }
    replace_rows(&path, &original)?;
    drop(crate::fixtures::open(&path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn complete_snapshot_restores_current_history_metadata_and_original_receipt() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::transaction::RecordBatchTransactionOutcome;
    use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};
    let directory = crate::fixtures::directory("metadata-native-snapshot")?;
    let source = directory.join("source");
    let target = directory.join("target");
    let mut store = fresh(&source)?;
    let first = metadata(7);
    let second = metadata(8);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&first)])?,
        0,
    )?;
    let changed = encode(
        &transition(&store, b"i", b"transition", 101, b"q", b"next")?,
        &[Some(&second)],
    )?;
    let RecordBatchTransactionOutcome::Retained {
        outcome: original, ..
    } = apply(&mut store, &changed, 1)?
    else {
        return Err("retained original".into());
    };
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(store);
    let events = history_ids(&source)?;
    let scope = crate::fixtures::scope()?;
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = fresh(&target)?;
    assert_eq!(
        restored
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    assert_eq!(
        restored.workflow_record_metadata(&scope, &p, &id, None, f::read_limits())?,
        Some(second.clone())
    );
    assert_eq!(
        restored.workflow_record_metadata(&scope, &p, &id, Some(&events[0]), f::read_limits())?,
        Some(first)
    );
    assert_eq!(
        restored.workflow_record_metadata(&scope, &p, &id, Some(&events[1]), f::read_limits())?,
        Some(second)
    );
    let RecordBatchTransactionOutcome::Retained {
        outcome: replay, ..
    } = apply(&mut restored, &changed, 2)?
    else {
        return Err("retained replay".into());
    };
    assert_eq!(replay, original);
    drop(restored);
    drop(crate::fixtures::open(&target)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn metadata_snapshot_cannot_implicitly_upgrade_a_scalar_binding_profile() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};
    let directory = crate::fixtures::directory("metadata-native-snapshot-profile")?;
    let mut source = fresh(&directory.join("source"))?;
    let data = metadata(7);
    apply(
        &mut source,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    let mut wire = Vec::new();
    source.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let target = directory.join("target");
    drop(legacy_fresh(&target)?.upgrade_history_snapshot_layout()?);
    let before = crate::fixtures::raw_rows(&target)?;
    let manifest = f::raw_manifest(&target)?;
    let scope = crate::fixtures::scope()?;
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut legacy = crate::fixtures::open(&target)?;
    assert!(
        legacy
            .install_snapshot(stage, SnapshotLimits::default())
            .is_err()
    );
    drop((source, legacy));
    assert_eq!(crate::fixtures::raw_rows(&target)?, before);
    assert_eq!(f::raw_manifest(&target)?, manifest);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn uncertain_commit_reopens_only_a_complete_old_or_new_metadata_cohort() -> TestResult {
    use super::super::claim::fixtures as claim;
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };
    use wf_ports::state::StateTransactionError;
    use wf_storage::transaction::{
        RecordBatchTransactionError, WorkflowValueTransactionError,
        apply_workflow_metadata_batch_transaction,
    };
    let directory = crate::fixtures::directory("metadata-native-uncertain")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let first = metadata(7);
    let second = metadata(8);
    let created = encode(&request(b"i", b"create", 100)?, &[Some(&first)])?;
    apply(&mut store, &created, 0)?;
    let changed = encode(
        &transition(&store, b"i", b"transition", 101, b"q", b"next")?,
        &[Some(&second)],
    )?;
    let mut healthy = fresh(&directory.join("healthy"))?;
    apply(&mut healthy, &created, 0)?;
    apply(&mut healthy, &changed, 1)?;
    drop((store, healthy));
    let fault = Arc::new(AtomicU8::new(0));
    let mut wrapped = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    // Skip successful engine growth syncs; fail the delegated publication sync.
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_workflow_metadata_batch_transaction(
            &mut wrapped,
            &crate::fixtures::scope()?,
            changed.as_bytes(),
            crate::fixtures::log(1),
            f::limits()
        )
        .err(),
        Some(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain)
        ))
    );
    assert!(matches!(
        wrapped.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        ),
        Err(StateTransactionError::Unavailable)
    ));
    drop(wrapped);
    let mut reopened = crate::fixtures::open(&path)?;
    let version = claim::record(&mut reopened, b"i")?.record.version();
    assert!([1, 2].contains(&version));
    assert_eq!(
        reopened.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        )?,
        Some(if version == 1 { first } else { second.clone() })
    );
    apply(&mut reopened, &changed, version)?;
    assert_eq!(
        reopened.workflow_record_metadata(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            None,
            f::read_limits()
        )?,
        Some(second)
    );
    drop(reopened);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
