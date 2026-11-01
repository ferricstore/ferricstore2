//! Inclusive independent query boundaries; failures leave authority healthy.

use super::*;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{GroupId, RecoveryId, TenantId};

#[test]
fn native_record_query_accounts_for_all_five_inclusive_resource_boundaries() -> TestResult {
    let directory = crate::fixtures::directory("record-query-resources")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let data = metadata(7);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let observed = store.read_workflow_record(&scope, &p, &id, Query::default(), limits())?;
    let used = [
        observed.row_reads,
        observed.inspected_bytes,
        observed.returned_bytes,
        observed.work_units,
        observed.temporary_bytes,
    ];
    assert!(used.into_iter().all(|v| v > 0));
    for (index, amount) in used.into_iter().enumerate() {
        for (at, success) in [(0, false), (amount - 1, false), (amount, true)] {
            let mut cap = limits();
            match index {
                0 => cap.maximum_row_reads = at,
                1 => cap.maximum_inspected_bytes = at,
                2 => cap.maximum_returned_bytes = at,
                3 => cap.maximum_work = at,
                _ => cap.maximum_temporary_bytes = at,
            }
            let result = store.read_workflow_record(&scope, &p, &id, Query::default(), cap);
            if success {
                assert!(result.is_ok(), "dimension {index} at {at}: {result:?}");
            } else {
                assert!(
                    matches!(result, Err(Error::Limit)),
                    "dimension {index} at {at}: {result:?}"
                );
            }
        }
    }
    let after = store.read_workflow_record(&scope, &p, &id, Query::default(), limits())?;
    assert_eq!(after.control, observed.control);
    assert_eq!(after.work_units, observed.work_units);
    assert_eq!(after.capture.ok_or("capture after refusal")?.metadata, data);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_query_counts_absent_probes_and_preserves_scope_boundaries() -> TestResult {
    let directory = crate::fixtures::directory("record-query-scope")?;
    let store = fresh(&directory.join("state"))?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let observed = store.read_workflow_record(&scope, &p, &id, Query::default(), limits())?;
    assert!(observed.capture.is_none());
    assert_eq!(observed.row_reads, 4);
    let mut cap = limits();
    cap.maximum_row_reads = 3;
    assert!(matches!(
        store.read_workflow_record(&scope, &p, &id, Query::default(), cap),
        Err(Error::Limit)
    ));
    let other_tenant = RequestScope::new(
        TenantId::new(b"other".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    assert!(
        store
            .read_workflow_record(&other_tenant, &p, &id, Query::default(), limits())?
            .capture
            .is_none()
    );
    let other_group = RequestScope::new(
        scope.tenant().clone(),
        GroupId::new(b"other".to_vec())?,
        scope.recovery().clone(),
    );
    let other_recovery = RequestScope::new(
        scope.tenant().clone(),
        scope.group().clone(),
        RecoveryId::new(b"other".to_vec())?,
    );
    for other in [&other_group, &other_recovery] {
        assert!(matches!(
            store.read_workflow_record(other, &p, &id, Query::default(), limits()),
            Err(Error::InvalidChanges)
        ));
    }
    for event in ["", "not-an-event", "1000-01", "-1-1"] {
        let query = Query {
            history: Some(event),
            expected_control: None,
        };
        assert!(
            matches!(
                store.read_workflow_record(&scope, &p, &id, query, limits()),
                Err(Error::InvalidChanges)
            ),
            "{event}"
        );
    }
    assert!(
        store
            .read_workflow_record(&scope, &p, &id, Query::default(), limits())
            .is_ok()
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_query_owned_results_survive_reopen_and_leave_all_rows_unchanged() -> TestResult {
    let directory = crate::fixtures::directory("record-query-owner")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let data = metadata(7);
    let encoded = encode(&request(b"i", b"create", 100)?, &[Some(&data)])?;
    let original = apply(&mut store, &encoded, 0)?;
    let retry = apply(&mut store, &encoded, 1)?;
    let wf_storage::transaction::RecordBatchTransactionOutcome::Retained { outcome: first, .. } =
        original
    else {
        return Err("original outcome".into());
    };
    let wf_storage::transaction::RecordBatchTransactionOutcome::Retained {
        outcome: repeated, ..
    } = retry
    else {
        return Err("retry outcome".into());
    };
    assert_eq!(first, repeated);
    drop(store);
    let before = crate::fixtures::raw_rows(&path)?;
    let events = history_ids(&path)?;
    let store = crate::fixtures::open(&path)?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let current = store.read_workflow_record(&scope, &p, &id, Query::default(), limits())?;
    let past = store.read_workflow_record(
        &scope,
        &p,
        &id,
        Query {
            history: Some(&events[0]),
            expected_control: Some(current.control),
        },
        limits(),
    )?;
    drop((store, scope, events));
    let current = current.capture.ok_or("current owned capture")?;
    let past = past.capture.ok_or("past owned capture")?;
    assert_eq!(current.record, past.record);
    assert_eq!(current.metadata, data);
    assert_eq!(past.metadata, data);
    assert!(current.values.lifecycle == past.values.lifecycle);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_query_captures_survive_complete_snapshot_transfer() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};
    let directory = crate::fixtures::directory("record-query-transfer")?;
    let source_path = directory.join("source");
    let target_path = directory.join("target");
    let mut source = fresh(&source_path)?;
    let first = metadata(7);
    let second = metadata(8);
    apply(
        &mut source,
        &encode(&request(b"i", b"create", 100)?, &[Some(&first)])?,
        0,
    )?;
    let changed = transition(&source, b"i", b"move", 101, b"q", b"next")?;
    apply(&mut source, &encode(&changed, &[Some(&second)])?, 1)?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let expected = source.read_workflow_record(&scope, &p, &id, Query::default(), limits())?;
    let mut wire = Vec::new();
    let captured = source.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(source);
    let events = history_ids(&source_path)?;
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut target = fresh(&target_path)?;
    assert_eq!(
        target
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    drop(target);
    let target = crate::fixtures::open(&target_path)?;
    let current = target.read_workflow_record(
        &scope,
        &p,
        &id,
        Query {
            history: None,
            expected_control: Some(expected.control),
        },
        limits(),
    )?;
    assert_eq!(current.control, expected.control);
    let actual = current.capture.ok_or("restored current")?;
    let expected = expected.capture.ok_or("original current")?;
    assert_eq!(actual.record, expected.record);
    assert_eq!(actual.metadata, second);
    assert!(actual.values.lifecycle == expected.values.lifecycle);
    for (index, event) in events.iter().enumerate() {
        let old = target
            .read_workflow_record(
                &scope,
                &p,
                &id,
                Query {
                    history: Some(event),
                    expected_control: Some(current.control),
                },
                limits(),
            )?
            .capture
            .ok_or("restored history")?;
        assert_eq!(old.record.record.version(), u64::try_from(index)? + 1);
        assert_eq!(old.metadata, if index == 0 { &first } else { &second }[..]);
        assert_eq!(
            old.values.lifecycle.incarnation,
            actual.values.lifecycle.incarnation
        );
    }
    drop(target);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
