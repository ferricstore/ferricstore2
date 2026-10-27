//! Real consumed publication/read observations feed the unchanged projector.

use wf_domain::history::values::{
    HistoryValue, HistoryValueEntry, HistoryValueLimits, HistoryValueReference as Ref,
    HistoryValueReturn, HistoryValueWrite, apply_history_value_results, prepare_history_values,
};
use wf_domain::value::{OriginalValueContext, OriginalValueLimits, OriginalValueNode};
use wf_ports::blob::{
    HistoryValueReadConversionError as ConversionError, HistoryValueReadConversionLimits,
    convert_history_value_reads,
};

use super::{
    HlcTimestamp, OriginalValueRefKind, RedbStateStore, StateStoreLimits, StateTransactionError,
    TestResult, WorkflowValueReadFailure, f, publish, publish_steps, read_clock, reference, setup,
    unsupported_refs,
};

fn conversion_limits() -> HistoryValueReadConversionLimits {
    HistoryValueReadConversionLimits {
        maximum_references: 64,
        maximum_work: 1_000_000,
        maximum_temporary_bytes: 8_000_000,
    }
}

fn projection_limits() -> HistoryValueLimits {
    HistoryValueLimits {
        maximum_entries: 16,
        maximum_references: 48,
        maximum_read_results: 48,
        maximum_writes: 96,
        maximum_work_units: 20_000_000,
        maximum_requested_bytes: 128_000_000,
        decoder: OriginalValueLimits {
            max_encoded_bytes: 262_144,
            max_nodes: 4_096,
            max_edges: 8_192,
            max_depth: 256,
            max_work_units: 200_000,
        },
    }
}

fn context() -> OriginalValueContext<'static> {
    OriginalValueContext {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    }
}

fn mixed_history<'a>(refs: &'a [Vec<u8>], missing: &'a [u8]) -> [HistoryValueEntry<'a, str>; 3] {
    [
        HistoryValueEntry {
            source: "event-one",
            references: [
                Ref::Binary(&refs[0]),
                Ref::Binary(&refs[1]),
                Ref::Binary(missing),
            ],
        },
        HistoryValueEntry {
            source: "event-two",
            references: [
                Ref::Binary(&refs[3]),
                Ref::Binary(&refs[2]),
                Ref::Binary(&refs[0]),
            ],
        },
        HistoryValueEntry {
            source: "event-three",
            references: [Ref::Binary(b"invalid"), Ref::Nil, Ref::NonBinary],
        },
    ]
}

fn assert_binary(writes: &[HistoryValueWrite], index: usize, expected: &[u8]) -> TestResult {
    let HistoryValue::Decoded(document) = &writes[index].value else {
        return Err("expected decoded binary".into());
    };
    assert!(
        matches!(document.root(), Some(OriginalValueNode::Binary { bytes }) if document.byte_range(bytes.clone()) == Some(expected))
    );
    Ok(())
}

#[test]
fn sized_value_reads_project_real_binary_and_stored_size() -> TestResult {
    let (directory, mut store, scope) = setup("sized-projection-binary")?;
    let owned = publish(
        &mut store,
        &scope,
        &[
            (b"magic", b"FSV2\x02opaque"),
            (b"small", b"xy"),
            (b"empty", b""),
            (b"large", b"12345678901234567"),
        ],
    )?;
    let missing = reference(b"missing", OriginalValueRefKind::Payload, 1)?;
    let history = mixed_history(&owned, &missing);
    let plan = prepare_history_values(
        &history,
        HistoryValueReturn::Enabled { max_bytes: 16 },
        projection_limits(),
    )?;
    assert_eq!(plan.references().len(), 6);
    let mut clock = read_clock(201)?;
    let batch = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        plan.references(),
        plan.read_threshold().ok_or("threshold")?,
        f::read_limits(),
    )?;
    let converted =
        convert_history_value_reads(batch, plan.references().len(), conversion_limits());
    assert!(converted.is_ok(), "{:?}", converted.as_ref().err());
    let overlay = apply_history_value_results(plan, Some(converted?), &context())?;
    assert_eq!(
        overlay
            .history
            .iter()
            .map(|entry| entry.source)
            .collect::<Vec<_>>(),
        ["event-one", "event-two", "event-three"]
    );
    let writes = overlay.writes;
    assert_eq!(
        writes
            .iter()
            .map(|w| (w.entry_index, w.field.key()))
            .collect::<Vec<_>>(),
        [
            (0, "payload"),
            (0, "payload_size"),
            (0, "result"),
            (0, "result_size"),
            (0, "error"),
            (0, "error_missing"),
            (1, "payload_omitted"),
            (1, "payload_size"),
            (1, "result"),
            (1, "result_size"),
            (1, "error"),
            (1, "error_size"),
            (2, "payload"),
            (2, "payload_missing"),
        ]
    );
    assert_eq!(clock.calls, 1);
    drop((store, scope, owned, missing));
    assert_binary(&writes, 0, b"FSV2\x02opaque")?;
    assert_binary(&writes, 2, b"xy")?;
    assert_binary(&writes, 8, b"")?;
    assert_binary(&writes, 10, b"FSV2\x02opaque")?;
    for (index, size) in [(1, 16), (3, 2), (7, 17), (9, 0), (11, 16)] {
        assert!(matches!(writes[index].value, HistoryValue::StoredSize(actual) if actual == size));
    }
    assert!(matches!(writes[4].value, HistoryValue::Nil));
    assert!(matches!(writes[5].value, HistoryValue::Boolean(true)));
    assert!(matches!(writes[6].value, HistoryValue::Boolean(true)));
    assert!(matches!(writes[12].value, HistoryValue::Nil));
    assert!(matches!(writes[13].value, HistoryValue::Boolean(true)));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_projection_keeps_drift_profiles_and_clock_errors() -> TestResult {
    let (directory, mut store, scope) = setup("sized-projection-refusals")?;
    let owned = publish_steps(&mut store, &scope, true)?;
    let refs = [&owned[0][..], &owned[1][..]];
    let mut clock = read_clock(249)?;
    let batch = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        1,
        f::read_limits(),
    )?;
    let first = convert_history_value_reads(batch, 2, conversion_limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    first?;
    clock.hlc = HlcTimestamp::new(1_500, 0)?;
    let batch = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        1,
        f::read_limits(),
    )?;
    assert!(matches!(
        convert_history_value_reads(batch, 2, conversion_limits()),
        Err(ConversionError::Unprojectable {
            reference_index: 1,
            failure: WorkflowValueReadFailure::UnsafeDrift
        })
    ));
    clock.hlc = HlcTimestamp::new(249, 0)?;
    let unsupported = unsupported_refs()?;
    let batch = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &[&unsupported[0], &unsupported[1]],
        1,
        f::read_limits(),
    )?;
    assert!(matches!(
        convert_history_value_reads(batch, 2, conversion_limits()),
        Err(ConversionError::Unprojectable {
            reference_index: 0,
            failure: WorkflowValueReadFailure::UnsupportedProfile
        })
    ));
    clock.failure = Some(StateTransactionError::Unavailable);
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 1, f::read_limits()),
        Err(StateTransactionError::Unavailable)
    ));
    clock.failure = None;
    let healthy = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        1,
        f::read_limits(),
    )?;
    assert!(convert_history_value_reads(healthy, 2, conversion_limits()).is_ok());
    assert_eq!(clock.calls, 5);
    drop((store, owned));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_projection_survives_reopen_and_owner_drop() -> TestResult {
    let (directory, mut store, scope) = setup("sized-projection-reopen")?;
    let owned = publish(&mut store, &scope, &[(b"i", b"xyz")])?;
    drop(store);
    let path = directory.join("state");
    let before = crate::fixtures::raw_rows(&path)?;
    let store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let history = [HistoryValueEntry {
        source: "retained event",
        references: [Ref::Binary(&owned[0]), Ref::Nil, Ref::Nil],
    }];
    let plan = prepare_history_values(
        &history,
        HistoryValueReturn::Enabled { max_bytes: 3 },
        projection_limits(),
    )?;
    let mut clock = read_clock(201)?;
    let batch = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        plan.references(),
        plan.read_threshold().ok_or("threshold")?,
        f::read_limits(),
    )?;
    let converted =
        convert_history_value_reads(batch, plan.references().len(), conversion_limits());
    assert!(converted.is_ok(), "{:?}", converted.as_ref().err());
    let overlay = apply_history_value_results(plan, Some(converted?), &context())?;
    assert_eq!(overlay.history[0].source, "retained event");
    let writes = overlay.writes;
    drop((store, scope, owned));
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].field.key(), "payload");
    assert_binary(&writes, 0, b"xyz")?;
    assert_eq!(writes[1].field.key(), "payload_size");
    assert!(matches!(writes[1].value, HistoryValue::StoredSize(3)));
    assert_eq!(clock.calls, 1);
    assert_eq!(crate::fixtures::raw_rows(&path)?, before);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
