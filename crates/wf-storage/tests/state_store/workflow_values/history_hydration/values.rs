//! Exact original field behavior over genuinely acquired historical values.

use super::*;

#[test]
fn history_hydration_reads_actual_stored_and_binary_history() -> TestResult {
    let directory = crate::fixtures::directory("hydration-values")?;
    let mut store = fresh(&directory.join("state"))?;
    let first = batch(b"binary", vec![create(b"i", Some(b"FSV2\x02binary"))?])?;
    f::apply_value(&mut store, &first.scope, &first, 0)?;
    apply(
        &mut store,
        &batch(
            b"stored",
            vec![
                transition(b"i", b"q", b"a", None)?,
                transition(b"i", b"a", b"b", Some(b"FSV2\x02\x83\x61\x07"))?,
                transition(b"i", b"b", b"c", Some(b"FSV2\x02bad"))?,
                transition(b"i", b"c", b"d", Some(b""))?,
                cancel(b"i", b"d", 5, b"FSV2\x02\x83\x61\x08")?,
            ],
        )?,
        1,
    )?;
    let mut clock = clock(101)?;
    let page = read(
        &store,
        &mut clock,
        b"i",
        selection(10, HistoryValueReturn::Enabled { max_bytes: 1_024 }),
    )?;
    assert_eq!(clock.calls, 1);
    assert_eq!(
        page.history
            .entries
            .iter()
            .map(|row| row.event.event_id.as_str())
            .collect::<Vec<_>>(),
        ["100-1", "100-2", "100-3", "100-4", "100-5", "100-6"]
    );
    assert!(page.history.lookahead.is_none());
    assert_eq!(
        page.writes
            .iter()
            .map(|w| (w.entry_index, w.field.key()))
            .collect::<Vec<_>>(),
        [
            (0, "payload"),
            (0, "payload_size"),
            (1, "payload"),
            (1, "payload_size"),
            (2, "payload"),
            (2, "payload_size"),
            (3, "payload_error"),
            (4, "payload"),
            (4, "payload_size"),
            (5, "payload"),
            (5, "payload_size"),
            (5, "error"),
            (5, "error_size"),
        ]
    );
    assert_mixed_fields(&page)?;
    let mut reverse = selection(2, HistoryValueReturn::Enabled { max_bytes: 1_024 });
    reverse.query.direction = WorkflowHistoryDirection::Descending;
    let reverse = read(&store, &mut clock, b"i", reverse)?;
    assert_eq!(reverse.history.continuation.as_deref(), Some("100-5"));
    assert_eq!(
        reverse
            .history
            .lookahead
            .as_ref()
            .ok_or("peek")?
            .event
            .event_id,
        "100-4"
    );
    assert!(matches!(
        field(&reverse, 2, "payload_error")?,
        HistoryValue::Error("ERR invalid flow value")
    ));
    drop(first);
    drop(store);
    assert!(page.projection_work_units > 0);
    assert!(page.projection_requested_bytes > 0);
    binary(&page, 0, "payload", b"FSV2\x02binary")?;
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_hydration_omits_exact_stored_sizes_and_pays_for_lookahead() -> TestResult {
    let directory = crate::fixtures::directory("hydration-omission")?;
    let mut store = fresh(&directory.join("state"))?;
    apply(
        &mut store,
        &batch(
            b"two",
            vec![
                create(b"i", Some(b"FSV2\x02\x83\x61\x07"))?,
                transition(b"i", b"q", b"a", Some(b""))?,
            ],
        )?,
        0,
    )?;
    let mut clock = clock(101)?;
    let page = read(
        &store,
        &mut clock,
        b"i",
        selection(1, HistoryValueReturn::Enabled { max_bytes: 7 }),
    )?;
    assert_eq!(page.history.entries.len(), 1);
    assert_eq!(
        page.history
            .lookahead
            .as_ref()
            .ok_or("peek")?
            .event
            .event_id,
        "100-2"
    );
    assert!(matches!(
        field(&page, 0, "payload_omitted")?,
        HistoryValue::Boolean(true)
    ));
    assert!(matches!(
        field(&page, 0, "payload_size")?,
        HistoryValue::StoredSize(8)
    ));
    binary(&page, 1, "payload", b"")?;
    let full = read(
        &store,
        &mut clock,
        b"i",
        selection(1, HistoryValueReturn::Enabled { max_bytes: 8 }),
    )?;
    assert!(matches!(
        document(&full, 0, "payload")?.root(),
        Some(OriginalValueNode::Integer(7))
    ));
    let negative = read(
        &store,
        &mut clock,
        b"i",
        selection(1, HistoryValueReturn::Enabled { max_bytes: -1 }),
    )?;
    for (entry, bytes) in [(0, 8), (1, 0)] {
        assert!(matches!(
            field(&negative, entry, "payload_omitted")?,
            HistoryValue::Boolean(true)
        ));
        assert!(
            matches!(field(&negative, entry, "payload_size")?, HistoryValue::StoredSize(size) if *size == bytes)
        );
    }
    let scope = crate::fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut short = limits();
    short.projection.maximum_entries = 1;
    let before = clock.calls;
    assert!(matches!(
        store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            selection(1, HistoryValueReturn::Enabled { max_bytes: 8 }),
            short
        ),
        Err(Error::Projection(
            wf_domain::history::values::HistoryValueError::Limit
        ))
    ));
    assert_eq!(clock.calls, before);
    short = limits();
    short.values.maximum_references = 1;
    assert!(matches!(
        store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            selection(1, HistoryValueReturn::Enabled { max_bytes: 8 }),
            short
        ),
        Err(Error::State(StateTransactionError::Limit))
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn assert_mixed_fields(page: &WorkflowHistoryValuePage) -> TestResult {
    binary(page, 0, "payload", b"FSV2\x02binary")?;
    binary(page, 1, "payload", b"FSV2\x02binary")?;
    assert_ne!(
        document(page, 0, "payload")?.encoded().as_ptr(),
        document(page, 1, "payload")?.encoded().as_ptr()
    );
    assert!(matches!(
        field(page, 0, "payload_size")?,
        HistoryValue::StoredSize(16)
    ));
    assert!(matches!(
        document(page, 2, "payload")?.root(),
        Some(OriginalValueNode::Integer(7))
    ));
    assert!(matches!(
        field(page, 2, "payload_size")?,
        HistoryValue::StoredSize(8)
    ));
    assert!(matches!(
        field(page, 3, "payload_error")?,
        HistoryValue::Error("ERR invalid flow value")
    ));
    assert!(
        !page
            .writes
            .iter()
            .any(|w| w.entry_index == 3 && w.field.key() == "payload")
    );
    binary(page, 4, "payload", b"")?;
    binary(page, 5, "payload", b"")?;
    assert!(matches!(
        field(page, 4, "payload_size")?,
        HistoryValue::StoredSize(0)
    ));
    assert!(matches!(
        document(page, 5, "error")?.root(),
        Some(OriginalValueNode::Integer(8))
    ));
    Ok(())
}
