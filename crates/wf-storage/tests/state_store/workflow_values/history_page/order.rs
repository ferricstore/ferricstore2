//! Original-ID ordering and exclusive cursor semantics on real history.

use wf_domain::workflow::{Cancel, Operation};
use wf_ports::consensus::PreparedRequest;
use wf_ports::log::encode_record_batch;
use wf_storage::transaction::apply_record_batch_transaction;

use super::*;

#[test]
fn history_page_reads_real_order_in_both_directions_and_boundaries() -> TestResult {
    let directory = crate::fixtures::directory("history-order")?;
    let mut store = fresh(&directory.join("state"))?;
    let scope = crate::fixtures::scope()?;
    let mut items = vec![create(b"i", Some(b"payload"))?];
    let mut from = b"q".to_vec();
    for i in 1..12 {
        let to = format!("s{i}").into_bytes();
        items.push(transition(b"i", &from, &to, None)?);
        from = to;
    }
    apply(&mut store, &batch(b"twelve", items)?, 0)?;
    let all = read(&store, &scope, b"i", query(20))?;
    let expected = [
        "100-1", "100-10", "100-11", "100-12", "100-2", "100-3", "100-4", "100-5", "100-6",
        "100-7", "100-8", "100-9",
    ];
    assert_eq!(ids(&all), expected);
    assert_eq!(all.record_version, Some(12));
    assert!(!all.has_more());
    assert!(all.continuation.is_none());
    for direction in [
        WorkflowHistoryDirection::Ascending,
        WorkflowHistoryDirection::Descending,
    ] {
        let expected: Vec<_> = if direction == WorkflowHistoryDirection::Ascending {
            expected.to_vec()
        } else {
            expected.iter().rev().copied().collect()
        };
        let mut actual = Vec::new();
        let mut boundary = None;
        loop {
            let mut selection = query(3);
            selection.direction = direction;
            selection.boundary_event = boundary.as_deref();
            let page = read(&store, &scope, b"i", selection)?;
            if let Some(next) = &page.lookahead {
                assert_eq!(
                    next.event.event_id,
                    expected[actual.len() + page.entries.len()]
                );
                assert_eq!(page.continuation.as_deref(), ids(&page).last().copied());
            }
            actual.extend(ids(&page).into_iter().map(str::to_owned));
            boundary = page.continuation;
            if boundary.is_none() {
                break;
            }
        }
        assert_eq!(actual, expected);
    }
    let mut selection = query(2);
    selection.boundary_event = Some("100-0");
    assert_eq!(ids(&read(&store, &scope, b"i", selection)?), &expected[..2]);
    selection.boundary_event = Some("100-100");
    assert_eq!(
        ids(&read(&store, &scope, b"i", selection)?),
        ["100-11", "100-12"]
    );
    for malformed in [
        "0100-1",
        "100-01",
        "100",
        "-1",
        "100-1-1",
        "18446744073709551616-1",
    ] {
        selection.boundary_event = Some(malformed);
        assert!(matches!(
            store.read_workflow_history_page(
                &scope,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
                selection,
                limits()
            ),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_page_keeps_every_historical_binding_and_legacy_visibility() -> TestResult {
    let directory = crate::fixtures::directory("history-binding")?;
    let mut store = fresh(&directory.join("state"))?;
    let scope = crate::fixtures::scope()?;
    let first = batch(
        b"binary-birth",
        vec![create(b"i", Some(b"FSV2\x02binary"))?],
    )?;
    f::apply_value(&mut store, &scope, &first, 0)?;
    let cancel = f::item(
        b"i",
        100,
        Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: Some(b"b".to_vec()),
            expected_version: Some(3),
            error: Some(Vec::new()),
        }),
    )?;
    apply(
        &mut store,
        &batch(
            b"three",
            vec![
                transition(b"i", b"q", b"a", None)?,
                transition(b"i", b"a", b"b", Some(b"FSV2\x02\x83\x61\x07"))?,
                cancel,
            ],
        )?,
        1,
    )?;
    let page = read(&store, &scope, b"i", query(10))?;
    assert_eq!(ids(&page), ["100-1", "100-2", "100-3", "100-4"]);
    assert_eq!(versions(&page.current_values)?, [Some(3), None, Some(4)]);
    let expected = [
        [Some(1), None, None],
        [Some(1), None, None],
        [Some(3), None, None],
        [Some(3), None, Some(4)],
    ];
    for (row, expected) in page.entries.iter().zip(expected) {
        assert_eq!(versions(&row.values)?, expected);
        let WorkflowValueBindingKnowledge::Known(Some(binding)) = &row.values else {
            return Err("historical provenance missing".into());
        };
        assert_eq!(binding.lifecycle.incarnation, 1);
    }
    let legacy_path = directory.join("legacy");
    let mut legacy = crate::fixtures::create(&legacy_path)?;
    let request = PreparedRequest {
        scope: scope.clone(),
        request: wf_types::identity::RequestId::new(b"legacy".to_vec())?,
        intent: wf_domain::workflow::request::ClientIntent::new(1, [0; 32])?,
        stamp: crate::fixtures::clock(100)?,
        operation: PreparedOperation::Batch(vec![create(b"old", None)?]),
    };
    let profile = crate::fixtures::driver_limits();
    let frame = encode_record_batch(request, profile.codec, profile.workspace_bytes)?;
    let applied = apply_record_batch_transaction(
        &mut legacy,
        &scope,
        frame.as_bytes(),
        crate::fixtures::log(0),
        profile,
    )?;
    assert!(matches!(
        applied,
        RecordBatchTransactionOutcome::Retained { .. }
    ));
    let old = read(&legacy, &scope, b"old", query(10))?;
    assert_eq!(ids(&old), ["100-1"]);
    assert!(matches!(
        old.current_values,
        WorkflowValueBindingKnowledge::LegacyUnavailable
    ));
    assert!(matches!(
        old.entries[0].values,
        WorkflowValueBindingKnowledge::LegacyUnavailable
    ));
    drop(legacy);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
