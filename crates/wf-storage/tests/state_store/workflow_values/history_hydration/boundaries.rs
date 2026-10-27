//! No-read boundaries, exact expiry/refusal distinctions and independent caps.

use wf_domain::history::values::HistoryValueError;
use wf_domain::value::OriginalFlowValueError;
use wf_ports::blob::HistoryValueReadConversionError;
use wf_ports::consensus::PreparedRequest;
use wf_ports::log::encode_record_batch;
use wf_storage::transaction::apply_record_batch_transaction;

use super::*;

fn legacy(path: &Path) -> TestResult<RedbStateStore> {
    let mut store = crate::fixtures::create(path)?;
    let scope = crate::fixtures::scope()?;
    let profile = crate::fixtures::driver_limits();
    let request = PreparedRequest {
        scope: scope.clone(),
        request: wf_types::identity::RequestId::new(b"legacy-hydration".to_vec())?,
        intent: wf_domain::workflow::request::ClientIntent::new(1, [0; 32])?,
        stamp: crate::fixtures::clock(100)?,
        operation: PreparedOperation::Batch(vec![create(b"i", Some(b"old"))?]),
    };
    let frame = encode_record_batch(request, profile.codec, profile.workspace_bytes)?;
    assert!(matches!(
        apply_record_batch_transaction(
            &mut store,
            &scope,
            frame.as_bytes(),
            crate::fixtures::log(0),
            profile
        )?,
        RecordBatchTransactionOutcome::Retained { .. }
    ));
    Ok(store)
}

#[test]
fn history_hydration_preserves_disabled_empty_legacy_and_clock_boundaries() -> TestResult {
    let directory = crate::fixtures::directory("hydration-boundaries")?;
    let mut store = fresh(&directory.join("state"))?;
    apply(
        &mut store,
        &batch(b"birth", vec![create(b"i", Some(b"raw"))?])?,
        0,
    )?;
    let mut clock = clock(101)?;
    read(
        &store,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
    )?;
    let before = clock.calls;
    clock.failure = Some(StateTransactionError::Corrupt);
    let disabled = read(
        &store,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Disabled),
    )?;
    assert_eq!(disabled.history.entries.len(), 1);
    assert!(disabled.writes.is_empty());
    assert_eq!(disabled.projection_work_units, 0);
    let missing = read(
        &store,
        &mut clock,
        b"absent",
        selection(
            10,
            HistoryValueReturn::Enabled {
                max_bytes: i128::MAX,
            },
        ),
    )?;
    assert!(missing.history.record_version.is_none());
    assert!(missing.writes.is_empty());
    assert_eq!(clock.calls, before);
    let scope = crate::fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    assert!(matches!(
        store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
            limits()
        ),
        Err(Error::Clock(StateTransactionError::Corrupt))
    ));
    clock.failure = None;
    read(
        &store,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
    )?;
    let old = legacy(&directory.join("legacy"))?;
    let before = clock.calls;
    let disabled = read(
        &old,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Disabled),
    )?;
    assert_eq!(disabled.history.entries[0].event.event_id, "100-1");
    assert!(disabled.writes.is_empty());
    assert!(matches!(
        old.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
            limits()
        ),
        Err(Error::State(StateTransactionError::UnsupportedLayout))
    ));
    assert_eq!(clock.calls, before);
    drop(old);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_hydration_keeps_expiry_drift_and_structured_decode_refusals() -> TestResult {
    let directory = crate::fixtures::directory("hydration-expiry")?;
    let mut store = fresh(&directory.join("state"))?;
    let mut request = batch(
        b"terminal",
        vec![create(b"i", Some(b"raw"))?, cancel(b"i", b"q", 1, b"")?],
    )?;
    request.inputs[1].terminal_ttl_ms = Some(50);
    apply(&mut store, &request, 0)?;
    let mut clock = clock(149)?;
    let page = read(
        &store,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
    )?;
    binary(&page, 1, "error", b"")?;
    clock.hlc = HlcTimestamp::new(150, 0)?;
    clock.wall = TimestampMillis::new(150);
    let expired = read(
        &store,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
    )?;
    assert!(matches!(field(&expired, 1, "error")?, HistoryValue::Nil));
    assert!(matches!(
        field(&expired, 1, "error_missing")?,
        HistoryValue::Boolean(true)
    ));
    binary(&expired, 1, "payload", b"raw")?;
    clock.hlc = HlcTimestamp::new(1_500, 0)?;
    clock.wall = TimestampMillis::new(149);
    let scope = crate::fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    assert!(matches!(
        store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &RecordId::new(b"i".to_vec())?,
            selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
            limits()
        ),
        Err(Error::Conversion(
            HistoryValueReadConversionError::Unprojectable {
                reference_index: 1,
                failure: WorkflowValueReadFailure::UnsafeDrift
            }
        ))
    ));
    apply(
        &mut store,
        &batch(
            b"atom",
            vec![create(b"j", Some(b"FSV2\x02\x83\x77\x07unknown"))?],
        )?,
        1,
    )?;
    clock.hlc = HlcTimestamp::new(101, 0)?;
    clock.wall = TimestampMillis::new(101);
    assert!(matches!(
        store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &RecordId::new(b"j".to_vec())?,
            selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
            limits()
        ),
        Err(Error::Projection(HistoryValueError::Decode(
            OriginalFlowValueError::UnsupportedContext {
                tag: 119,
                offset: 6
            }
        )))
    ));
    let mut captured = selection(10, HistoryValueReturn::Enabled { max_bytes: 100 });
    captured.decode_context.safe_atoms = Some(&[]);
    let absent = read(&store, &mut clock, b"j", captured)?;
    assert!(matches!(
        field(&absent, 0, "payload_error")?,
        HistoryValue::Error("ERR invalid flow value")
    ));
    assert!(!absent.writes.iter().any(|w| w.field.key() == "payload"));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn short(case: usize) -> WorkflowHistoryHydrationLimits {
    let mut l = limits();
    match case {
        0 => l.history.maximum_row_reads = 1,
        1 => l.history.maximum_inspected_bytes = 1,
        2 => l.history.maximum_returned_bytes = 1,
        3 => l.history.maximum_work = 1,
        4 => l.history.maximum_temporary_bytes = 1,
        5 => l.values.maximum_references = 1,
        6 => l.values.maximum_stored_bytes = 1,
        7 => l.values.maximum_work = 1,
        8 => l.values.maximum_temporary_bytes = 1,
        9 => l.conversion.maximum_references = 1,
        10 => l.conversion.maximum_work = 1,
        11 => l.conversion.maximum_temporary_bytes = 1,
        12 => l.projection.maximum_entries = 1,
        13 => l.projection.maximum_references = 1,
        14 => l.projection.maximum_work_units = 1,
        15 => l.projection.maximum_requested_bytes = 1,
        16 => l.maximum_work = 1,
        17 => l.maximum_temporary_bytes = 1,
        18 => {
            l.maximum_work =
                l.history.maximum_work + l.values.maximum_work + l.conversion.maximum_work;
        }
        _ => l.maximum_temporary_bytes = l.values.maximum_temporary_bytes,
    }
    l
}

#[test]
fn history_hydration_enforces_all_stage_and_composition_budgets() -> TestResult {
    let directory = crate::fixtures::directory("hydration-caps")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    apply(
        &mut store,
        &batch(
            b"two",
            vec![
                create(b"i", Some(b"first"))?,
                transition(b"i", b"q", b"a", Some(b"second"))?,
            ],
        )?,
        0,
    )?;
    let mut clock = clock(101)?;
    read(
        &store,
        &mut clock,
        b"i",
        selection(1, HistoryValueReturn::Enabled { max_bytes: 100 }),
    )?;
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    let meta = metadata(&path)?;
    let store = crate::fixtures::open(&path)?;
    let scope = crate::fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    for case in 0..20 {
        let result = store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            selection(1, HistoryValueReturn::Enabled { max_bytes: 100 }),
            short(case),
        );
        let error = result.err().ok_or("short budget accepted")?;
        match case {
            9..=11 => assert!(
                matches!(
                    error,
                    Error::Conversion(HistoryValueReadConversionError::Limit)
                ),
                "{case}: {error:?}"
            ),
            12..=15 => assert!(
                matches!(error, Error::Projection(HistoryValueError::Limit)),
                "{case}: {error:?}"
            ),
            _ => assert!(
                matches!(error, Error::State(StateTransactionError::Limit)),
                "{case}: {error:?}"
            ),
        }
        let funded = read(
            &store,
            &mut clock,
            b"i",
            selection(1, HistoryValueReturn::Enabled { max_bytes: 100 }),
        )?;
        binary(&funded, 0, "payload", b"first")?;
        binary(&funded, 1, "payload", b"second")?;
    }
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    assert_eq!(metadata(&path)?, meta);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
