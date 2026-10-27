//! Pinned source conformance and logical ingress through actual publication.
//! The first two cases follow implementation; logical preparation is tests-first.

use super::*;

use wf_domain::value::{
    OriginalValueEncodeLimits, OriginalValueInput, prepare_original_flow_value,
};

#[test]
fn history_hydration_preserves_original_file_reference_size() -> TestResult {
    // HistoryValuesTest bdafcf374348156d: stored size100, return cap10.
    // The real Rust alias replaces the source helper's arbitrary "ref".
    let directory = crate::fixtures::directory("hydration-source-size")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let payload = [b'x'; 100];
    apply(
        &mut store,
        &batch(b"source-size", vec![create(b"i", Some(&payload))?])?,
        0,
    )?;
    let scope = crate::fixtures::scope()?;
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let page = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 10 }),
        )?;
        assert_eq!(page.history.entries.len(), 1);
        assert!(matches!(
            field(&page, 0, "payload_omitted")?,
            HistoryValue::Boolean(true)
        ));
        assert!(matches!(
            field(&page, 0, "payload_size")?,
            HistoryValue::StoredSize(100)
        ));
        assert!(
            !page
                .writes
                .iter()
                .any(|write| write.field.key() == "payload")
        );
        let full = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
        )?;
        binary(&full, 0, "payload", &payload)?;
        assert!(matches!(
            field(&full, 0, "payload_size")?,
            HistoryValue::StoredSize(100)
        ));
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_hydration_preserves_original_corrupted_typed_value() -> TestResult {
    // HistoryValuesTest381f48c9d21d1d52: the exact source bytes and cap1024.
    let directory = crate::fixtures::directory("hydration-source-corrupt")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let corrupted = b"FSV2\x02not-an-external-term";
    apply(
        &mut store,
        &batch(b"source-corrupt", vec![create(b"i", Some(corrupted))?])?,
        0,
    )?;
    let scope = crate::fixtures::scope()?;
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let page = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 1_024 }),
        )?;
        assert_eq!(page.history.entries.len(), 1);
        assert!(matches!(
            field(&page, 0, "payload_error")?,
            HistoryValue::Error("ERR invalid flow value")
        ));
        assert!(
            !page
                .writes
                .iter()
                .any(|write| write.field.key() == "payload")
        );
        let omitted = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 10 }),
        )?;
        assert!(matches!(
            field(&omitted, 0, "payload_omitted")?,
            HistoryValue::Boolean(true)
        ));
        assert!(matches!(
            field(&omitted, 0, "payload_size")?,
            HistoryValue::StoredSize(bytes) if *bytes == corrupted.len()
        ));
        assert!(
            !omitted
                .writes
                .iter()
                .any(|write| write.field.key() == "payload_error")
        );
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_hydration_prepares_and_publishes_logical_typed_values() -> TestResult {
    let nodes = [
        OriginalValueNode::Binary { bytes: 0..9 },
        OriginalValueNode::Integer(7),
        OriginalValueNode::Tuple { children: 0..2 },
    ];
    let limits = OriginalValueEncodeLimits {
        max_input_bytes: 4_096,
        max_nodes: 1_024,
        max_edges: 2_048,
        max_depth: 128,
        max_encoded_bytes: 65_536,
        max_work_units: 1_000_000,
        max_temporary_bytes: 1_000_000,
    };
    let typed = prepare_original_flow_value(
        OriginalValueInput {
            bytes: b"FSV2\x02data",
            nodes: &nodes,
            edges: &[1, 0],
            root: 2,
        },
        limits,
    );
    assert!(
        typed.is_ok(),
        "healthy logical value preparation: {:?}",
        typed.as_ref().err()
    );
    let typed = typed?;
    let expected = b"FSV2\x02\x83h\x02a\x07m\0\0\0\x09FSV2\x02data";
    assert_eq!(typed.as_stored_bytes(), expected);
    let binary_nodes = [OriginalValueNode::Binary {
        bytes: 0..expected.len(),
    }];
    let raw = prepare_original_flow_value(
        OriginalValueInput {
            bytes: expected,
            nodes: &binary_nodes,
            edges: &[],
            root: 0,
        },
        limits,
    )?;
    let items = [(b"typed".as_slice(), typed), (b"binary".as_slice(), raw)]
        .into_iter()
        .map(|(id, value)| logical_create(id, value))
        .collect::<TestResult<Vec<_>>>()?;
    let directory = crate::fixtures::directory("hydration-logical-typed")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    apply(&mut store, &batch(b"logical-typed", items)?, 0)?;
    let scope = crate::fixtures::scope()?;
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let page = read(
            &store,
            &mut clock,
            b"typed",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 1024 }),
        )?;
        let value = document(&page, 0, "payload")?;
        let Some(OriginalValueNode::Tuple { children }) = value.root() else {
            return Err("typed tuple root".into());
        };
        let ids = value.edge_range(children.clone()).ok_or("tuple children")?;
        assert_eq!(ids.len(), 2);
        assert!(matches!(
            value.node(ids[0]),
            Some(OriginalValueNode::Integer(7))
        ));
        assert!(
            matches!(value.node(ids[1]), Some(OriginalValueNode::Binary { bytes })
            if value.byte_range(bytes.clone()) == Some(b"FSV2\x02data".as_slice()))
        );
        let page = read(
            &store,
            &mut clock,
            b"binary",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 1024 }),
        )?;
        binary(&page, 0, "payload", expected)?;
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn logical_create(
    id: &[u8],
    value: wf_domain::value::PreparedOriginalFlowValue,
) -> TestResult<PreparedBatchItem> {
    f::item(
        id,
        100,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(value.into_stored_bytes()),
        }),
    )
}
