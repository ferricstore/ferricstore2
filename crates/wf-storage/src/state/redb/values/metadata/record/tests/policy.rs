//! Native policy reads share one actual committed record revision.
use super::*;
use wf_domain::policy::complete::PolicySchemaLimits;
use wf_ports::state::{
    WorkflowRecordPolicyObservation as Policy, WorkflowRecordPolicyReadLimits,
    WorkflowRecordPolicyReadPort,
};
mod fixtures;
use fixtures::fixture_data::{check_complete, publish_original, publish_policy};
fn caps(record: wf_ports::state::WorkflowRecordReadLimits) -> WorkflowRecordPolicyReadLimits {
    WorkflowRecordPolicyReadLimits {
        record,
        schema: PolicySchemaLimits::default(),
    }
}

#[test]
fn native_record_policy_distinguishes_absence_from_an_unread_missing_record() -> TestResult {
    let (directory, store, scope) = setup("policy-absent")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let snapshot = store.read_workflow_record_with_policy(
        &scope,
        &partition,
        &id,
        Query::default(),
        caps(limits()),
    )?;
    assert!(matches!(snapshot.current_policy, Policy::Absent));
    assert_eq!(snapshot.record.row_reads, 5);
    let missing = store.read_workflow_record_with_policy(
        &scope,
        &partition,
        &RecordId::new(b"missing".to_vec())?,
        Query::default(),
        caps(limits()),
    )?;
    assert!(matches!(missing.current_policy, Policy::NotRead));
    assert!(missing.record.capture.is_none());
    assert_eq!(missing.record.row_reads, 4);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_charges_the_policy_probe_in_the_whole_read_budget() -> TestResult {
    let (directory, store, scope) = setup("policy-probe")?;
    let mut cap = limits();
    cap.maximum_row_reads = 4;
    assert!(matches!(
        store.read_workflow_record_with_policy(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            Query::default(),
            caps(cap),
        ),
        Err(Error::Limit)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_refuses_a_corrupt_same_scope_policy_and_fences() -> TestResult {
    let (directory, store, scope) = setup("policy-corrupt")?;
    let key = crate::state::key::encode_policy_key(&scope, b"r", 1024)?;
    rewrite(&store, false, &key, Some(b"corrupt"))?;
    assert!(matches!(
        store.read_workflow_record_with_policy(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            Query::default(),
            caps(limits()),
        ),
        Err(Error::Corrupt)
    ));
    assert!(matches!(
        store.read_workflow_record(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            Query::default(),
            limits(),
        ),
        Err(Error::Unavailable)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_pins_actual_policy_generation_across_consumed_publication() -> TestResult {
    let (directory, mut store, scope) = setup("policy-pinned")?;
    publish_policy(&mut store, &scope, 101, None, 1000, b"r")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut pinned = read::open(&store, &scope, &partition, &id, Query::default(), limits())?;
    let before = pinned.control(&scope)?;
    let original = row::capture(&mut pinned, &scope, &partition, &id, None)?.ok_or("old core")?;
    publish_policy(&mut store, &scope, 102, Some(1), 2000, b"r")?;
    let old_policy = super::super::policy::capture_policy(
        &mut pinned,
        &scope,
        b"r",
        PolicySchemaLimits::default(),
    )?;
    check_complete(old_policy, 1, 1000)?;
    assert_eq!(pinned.control(&scope)?, before);
    let mut limited = limits();
    limited.maximum_row_reads = 2;
    assert!(matches!(
        store.read_workflow_record_with_policy(
            &scope,
            &partition,
            &id,
            Query {
                history: None,
                expected_control: Some(before)
            },
            caps(limited),
        ),
        Err(Error::KnownAborted)
    ));
    let current = store.read_workflow_record_with_policy(
        &scope,
        &partition,
        &id,
        Query::default(),
        caps(limits()),
    )?;
    assert_ne!(current.record.control, before);
    check_complete(current.current_policy, 2, 2000)?;
    let current = current.record.capture.ok_or("current core")?;
    assert_eq!(
        current.record.record.version(),
        original.record.record.version()
    );
    assert_eq!(current.metadata, original.metadata);
    assert!(current.values.lifecycle == original.values.lifecycle);
    assert_eq!(current.values.lifecycle.policy_generation, 0);
    assert_eq!(current.values.lifecycle.birth.max_active_ms(), None);
    drop((pinned, store));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_reads_actual_original_backing_without_changing_it() -> TestResult {
    let (directory, mut store, scope) = setup("policy-original")?;
    publish_original(&mut store, &scope, 101)?;
    let before = rows(&store, false)?;
    let key = crate::state::key::encode_policy_key(&scope, b"r", 1024)?;
    let actual = before.get(&key).ok_or("physical original policy")?;
    let captured = store.read_workflow_record_with_policy(
        &scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"i".to_vec())?,
        Query::default(),
        caps(limits()),
    )?;
    let Policy::Present(wf_ports::state::StoredPolicyValue::Original(bytes)) =
        captured.current_policy
    else {
        return Err("expected actual Original backing".into());
    };
    assert_eq!(&bytes, actual);
    assert_eq!(bytes.first(), Some(&131));
    assert_eq!(
        captured
            .record
            .capture
            .ok_or("native capture")?
            .values
            .lifecycle
            .policy_profile,
        0
    );
    assert_eq!(rows(&store, false)?, before);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_reads_current_policy_for_an_exact_historical_record() -> TestResult {
    let (directory, mut store, scope) = setup("policy-history")?;
    publish_policy(&mut store, &scope, 101, None, 1000, b"r")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let physical = rows(&store, false)?;
    let (_, bytes) = physical
        .iter()
        .find(|(k, _)| k.get(6) == Some(&5))
        .ok_or("history")?;
    let history = value::inspect_history_snapshot(
        bytes,
        &scope,
        &partition,
        &id,
        wf_domain::workflow::admission::AdmissionLimits::default(),
        1_048_576,
    )?;
    let event = std::str::from_utf8(history.event_id())?;
    let captured = store.read_workflow_record_with_policy(
        &scope,
        &partition,
        &id,
        Query {
            history: Some(event),
            expected_control: None,
        },
        caps(limits()),
    )?;
    check_complete(captured.current_policy, 1, 1000)?;
    let core = captured.record.capture.ok_or("history capture")?;
    assert_eq!(core.record.record.version(), 1);
    assert_eq!(core.values.lifecycle.policy_generation, 0);
    assert_eq!(core.metadata.last(), Some(&7));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_uses_the_actual_tenant_and_type_key() -> TestResult {
    let (directory, mut store, scope) = setup("policy-scope")?;
    let foreign = wf_domain::workflow::request::RequestScope::new(
        wf_types::identity::TenantId::new(b"other".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    publish_policy(&mut store, &foreign, 101, None, 1000, b"r")?;
    publish_policy(&mut store, &scope, 102, None, 2000, b"other")?;
    let captured = store.read_workflow_record_with_policy(
        &scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"i".to_vec())?,
        Query::default(),
        caps(limits()),
    )?;
    assert!(matches!(captured.current_policy, Policy::Absent));
    let wrong_key = crate::state::key::encode_policy_key(&scope, b"other", 1024)?;
    let target = crate::state::key::encode_policy_key(&scope, b"r", 1024)?;
    let physical = rows(&store, false)?;
    rewrite(
        &store,
        false,
        &target,
        Some(physical.get(&wrong_key).ok_or("wrong type donor")?),
    )?;
    assert!(matches!(
        store.read_workflow_record_with_policy(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            Query::default(),
            caps(limits())
        ),
        Err(Error::Corrupt)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_preserves_all_inclusive_whole_read_limits() -> TestResult {
    let (directory, mut store, scope) = setup("policy-limits")?;
    publish_policy(&mut store, &scope, 101, None, 1000, b"r")?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let baseline = store.read_workflow_record_with_policy(
        &scope,
        &p,
        &id,
        Query::default(),
        caps(limits()),
    )?;
    let used = [
        baseline.record.row_reads,
        baseline.record.inspected_bytes,
        baseline.record.returned_bytes,
        baseline.record.work_units,
        baseline.record.temporary_bytes,
    ];
    let before = rows(&store, false)?;
    for (dimension, amount) in used.into_iter().enumerate() {
        for (at, success) in [(amount, true), (amount - 1, false), (0, false)] {
            let mut cap = limits();
            match dimension {
                0 => cap.maximum_row_reads = at,
                1 => cap.maximum_inspected_bytes = at,
                2 => cap.maximum_returned_bytes = at,
                3 => cap.maximum_work = at,
                _ => cap.maximum_temporary_bytes = at,
            }
            let result = store.read_workflow_record_with_policy(
                &scope,
                &p,
                &id,
                Query::default(),
                caps(cap),
            );
            assert_eq!(result.is_ok(), success, "{dimension} at {at}: {result:?}");
        }
    }
    let mut cap = caps(limits());
    cap.schema = PolicySchemaLimits::new(cap.schema.policy_limits(), 1)?;
    assert!(matches!(
        store.read_workflow_record_with_policy(&scope, &p, &id, Query::default(), cap),
        Err(Error::Limit)
    ));
    assert_eq!(rows(&store, false)?, before);
    assert_eq!(
        store
            .read_workflow_record(&scope, &p, &id, Query::default(), limits())?
            .row_reads,
        4
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_policy_reopens_complete_and_original_current_authority() -> TestResult {
    for original in [false, true] {
        let (directory, mut store, scope) = setup(&format!("policy-reopen-{original}"))?;
        if original {
            publish_original(&mut store, &scope, 101)?;
        } else {
            publish_policy(&mut store, &scope, 101, None, 1000, b"r")?;
        }
        let p = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let before = store.read_workflow_record_with_policy(
            &scope,
            &p,
            &id,
            Query::default(),
            caps(limits()),
        )?;
        drop(store);
        let store = crate::state::RedbStateStore::open(
            &directory.join("state"),
            scope.group().clone(),
            scope.recovery().clone(),
            crate::state::StateStoreLimits::default(),
        )?;
        let after = store.read_workflow_record_with_policy(
            &scope,
            &p,
            &id,
            Query::default(),
            caps(limits()),
        )?;
        assert_eq!(before.record.control, after.record.control);
        assert_eq!(
            before.record.capture.ok_or("before")?.metadata,
            after.record.capture.ok_or("after")?.metadata
        );
        let Policy::Present(before) = before.current_policy else {
            return Err("before policy".into());
        };
        let Policy::Present(after) = after.current_policy else {
            return Err("after policy".into());
        };
        assert_eq!(before.encoded(), after.encoded());
        let mut excessive = caps(limits());
        excessive.schema = PolicySchemaLimits::new(
            excessive.schema.policy_limits(),
            excessive.schema.max_work_bytes() + 1,
        )?;
        assert!(matches!(
            store.read_workflow_record_with_policy(&scope, &p, &id, Query::default(), excessive),
            Err(Error::InvalidChanges)
        ));
        assert!(
            store
                .read_workflow_record_with_policy(&scope, &p, &id, Query::default(), caps(limits()))
                .is_ok()
        );
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}
