//! Real consumed writes feed a coherent native point query.

use super::{TestResult, fixtures::*};
use wf_ports::state::{
    StateTransactionError as Error, WorkflowRecordReadLimits, WorkflowRecordReadPort,
    WorkflowRecordReadQuery as Query,
};
use wf_types::identity::{PartitionId, RecordId};

mod corruption;
mod resources;

fn limits() -> WorkflowRecordReadLimits {
    WorkflowRecordReadLimits {
        maximum_row_reads: 16,
        maximum_inspected_bytes: 8 * crate::fixtures::MIB,
        maximum_returned_bytes: 8 * crate::fixtures::MIB,
        maximum_work: 64 * crate::fixtures::MIB,
        maximum_temporary_bytes: 16 * crate::fixtures::MIB,
    }
}

#[test]
fn native_record_read_returns_matching_current_core_metadata_and_lifecycle() -> TestResult {
    let directory = crate::fixtures::directory("record-query-current")?;
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
    let result = store.read_workflow_record(&scope, &p, &id, Query::default(), limits());
    assert!(result.is_ok(), "{result:?}");
    let result = result?;
    assert_eq!(result.row_reads, 4);
    let captured = result.capture.ok_or("missing native record capture")?;
    assert_eq!(captured.record.record.version(), 1);
    assert_eq!(captured.record.record.state(), b"q");
    assert_eq!(captured.metadata, data);
    assert_eq!(
        captured.values.lifecycle.incarnation,
        captured.record.ordering.state_enter_seq
    );
    assert!(captured.values.refs.payload.is_some());
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_read_preserves_exact_historical_postimages_after_transition() -> TestResult {
    let directory = crate::fixtures::directory("record-query-history")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let first = metadata(7);
    let second = metadata(8);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&first)])?,
        0,
    )?;
    let next = transition(&store, b"i", b"move", 101, b"q", b"next")?;
    apply(&mut store, &encode(&next, &[Some(&second)])?, 1)?;
    drop(store);
    let events = history_ids(&path)?;
    let store = crate::fixtures::open(&path)?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut birth = None;
    for (index, event) in events.iter().enumerate() {
        let result = store.read_workflow_record(
            &scope,
            &p,
            &id,
            Query {
                history: Some(event),
                expected_control: None,
            },
            limits(),
        );
        assert!(result.is_ok(), "{result:?}");
        let capture = result?.capture.ok_or("missing exact historical capture")?;
        assert_eq!(capture.record.record.version(), u64::try_from(index)? + 1);
        assert_eq!(
            capture.record.record.state(),
            if index == 0 { &b"q"[..] } else { &b"next"[..] }
        );
        assert_eq!(
            capture.metadata,
            if index == 0 { &first } else { &second }[..]
        );
        if let Some(expected) = birth {
            assert_eq!(capture.values.lifecycle.incarnation, expected);
        }
        birth = Some(capture.values.lifecycle.incarnation);
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_read_distinguishes_known_absence_from_unsupported_metadata() -> TestResult {
    let directory = crate::fixtures::directory("record-query-missing")?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let store = fresh(&directory.join("state"))?;
    let result = store.read_workflow_record(&scope, &p, &id, Query::default(), limits());
    assert!(result.is_ok(), "{result:?}");
    assert!(result?.capture.is_none());
    let legacy = legacy_fresh(&directory.join("legacy"))?;
    assert!(matches!(
        legacy.read_workflow_record(&scope, &p, &id, Query::default(), limits()),
        Err(Error::UnsupportedPolicyProfile)
    ));
    drop((store, legacy));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_record_read_control_precondition_refuses_a_changed_revision() -> TestResult {
    let directory = crate::fixtures::directory("record-query-control")?;
    let store = fresh(&directory.join("state"))?;
    let scope = crate::fixtures::scope()?;
    let p = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let before = store.read_workflow_record(&scope, &p, &id, Query::default(), limits());
    assert!(before.is_ok(), "{before:?}");
    let expected = before?.control;
    let same = Query {
        history: None,
        expected_control: Some(expected),
    };
    assert!(
        store
            .read_workflow_record(&scope, &p, &id, same, limits())
            .is_ok()
    );
    let changed = wf_domain::workflow::control::ControlState::initial(crate::fixtures::clock(1)?);
    let stale = Query {
        history: None,
        expected_control: Some(changed),
    };
    assert!(matches!(
        store.read_workflow_record(&scope, &p, &id, stale, limits()),
        Err(Error::KnownAborted)
    ));
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
fn native_record_read_moves_opaque_metadata_into_the_explicit_context_decoder() -> TestResult {
    use wf_domain::metadata::record::{
        OriginalRecordMetadataField, decode_original_record_metadata,
    };
    use wf_domain::value::{OriginalValueContext, OriginalValueLimits, OriginalValueNode};
    let directory = crate::fixtures::directory("record-query-decode-owner")?;
    let mut store = fresh(&directory.join("state"))?;
    let data = metadata(7);
    apply(
        &mut store,
        &encode(&request(b"i", b"create", 100)?, &[Some(&data)])?,
        0,
    )?;
    let captured = store
        .read_workflow_record(
            &crate::fixtures::scope()?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            Query::default(),
            limits(),
        )?
        .capture
        .ok_or("native capture")?;
    let pointer = captured.metadata.as_ptr();
    drop(store);
    let context = OriginalValueContext {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    };
    let decoded = decode_original_record_metadata(
        captured.metadata,
        &context,
        OriginalValueLimits {
            max_encoded_bytes: 65_536,
            max_nodes: 64,
            max_edges: 128,
            max_depth: 16,
            max_work_units: 1_048_576,
        },
    )?;
    assert_eq!(decoded.document().encoded().as_ptr(), pointer);
    assert_eq!(decoded.document().encoded(), data);
    assert!(matches!(
        decoded.field(OriginalRecordMetadataField::Attributes),
        Some(OriginalValueNode::Integer(7))
    ));
    assert!(
        decoded
            .field(OriginalRecordMetadataField::Partition)
            .is_none()
    );
    assert_eq!(captured.record.record.version(), 1);
    // This schema decode preserves the opaque integer; original attributes
    // normalization is a separate mandatory ingress validation, not inferred here.
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
