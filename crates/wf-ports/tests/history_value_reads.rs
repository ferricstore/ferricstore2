//! Consuming read conversion preserves observations and complete owned values.

#![forbid(unsafe_code)]

use std::error::Error;

use wf_domain::history::values::{
    HistoryValue, HistoryValueEntry, HistoryValueLimits, HistoryValueReadBatch as Batch,
    HistoryValueReadResult as Read, HistoryValueReference as Ref, HistoryValueReturn,
    apply_history_value_results, prepare_history_values,
};
use wf_domain::value::{OriginalValueContext, OriginalValueLimits, OriginalValueNode as Node};
use wf_ports::blob::{
    HistoryValueReadConversionError as ConversionError, HistoryValueReadConversionLimits,
    WorkflowValueReadFailure as Failure, WorkflowValueSizedReadBatch as SizedBatch,
    WorkflowValueSizedReadValue as Value, convert_history_value_reads,
};

fn limits() -> HistoryValueReadConversionLimits {
    HistoryValueReadConversionLimits {
        maximum_references: 64,
        maximum_work: 1_000_000,
        maximum_temporary_bytes: 1_000_000,
    }
}

fn healthy() -> SizedBatch {
    SizedBatch {
        values: vec![Value::Stored(b"x".to_vec())],
        slots: vec![Ok(Some(0))],
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

#[test]
fn history_read_conversion_preserves_order_and_moves_owners() -> Result<(), Box<dyn Error>> {
    let bytes = b"FSV2\x01FSV2\x02opaque".to_vec();
    let pointer = bytes.as_ptr();
    let input = SizedBatch {
        values: vec![
            Value::Stored(Vec::new()),
            Value::Omitted { stored_bytes: 100 },
            Value::Stored(bytes),
        ],
        slots: vec![
            Ok(Some(0)),
            Ok(None),
            Err(Failure::ReadFailed),
            Ok(Some(1)),
            Err(Failure::Unavailable),
            Ok(Some(2)),
        ],
    };
    let result = convert_history_value_reads(input, 6, limits());
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let Batch::Values(values) = result? else {
        return Err("expected ordered values".into());
    };
    assert_eq!(values.len(), 6);
    assert!(matches!(&values[0], Read::Stored(bytes) if bytes.is_empty()));
    assert!(matches!(&values[1], Read::Missing));
    assert!(matches!(
        &values[2],
        Read::StorageFailure { diagnostic: None }
    ));
    assert!(matches!(&values[3], Read::StoredSize { bytes: 100 }));
    assert!(matches!(
        &values[4],
        Read::StorageFailure { diagnostic: None }
    ));
    let Read::Stored(bytes) = &values[5] else {
        return Err("expected complete stored bytes".into());
    };
    assert_eq!(bytes.as_slice(), b"FSV2\x01FSV2\x02opaque");
    assert_eq!(bytes.as_ptr(), pointer);
    Ok(())
}

#[test]
fn history_read_conversion_rejects_malformed_owner_sequences() -> Result<(), Box<dyn Error>> {
    let first = convert_history_value_reads(healthy(), 1, limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    first?;
    let cases = [
        (
            SizedBatch {
                values: vec![],
                slots: vec![Ok(Some(0))],
            },
            1,
        ),
        (
            SizedBatch {
                values: vec![Value::Stored(vec![1])],
                slots: vec![],
            },
            0,
        ),
        (
            SizedBatch {
                values: vec![Value::Stored(vec![1])],
                slots: vec![Ok(None)],
            },
            1,
        ),
        (
            SizedBatch {
                values: vec![Value::Stored(vec![1])],
                slots: vec![Ok(Some(0)), Ok(Some(0))],
            },
            2,
        ),
        (
            SizedBatch {
                values: vec![Value::Stored(vec![1]), Value::Stored(vec![2])],
                slots: vec![Ok(Some(1)), Ok(Some(0))],
            },
            2,
        ),
        (
            SizedBatch {
                values: vec![Value::Stored(vec![1])],
                slots: vec![Ok(Some(2))],
            },
            1,
        ),
        (healthy(), 2),
    ];
    for (input, count) in cases {
        assert!(matches!(
            convert_history_value_reads(input, count, limits()),
            Err(ConversionError::InvalidBatch)
        ));
    }
    Ok(())
}

#[test]
fn history_read_conversion_keeps_unprojectable_failures_explicit() -> Result<(), Box<dyn Error>> {
    let first = convert_history_value_reads(healthy(), 1, limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    first?;
    for failure in [Failure::UnsafeDrift, Failure::UnsupportedProfile] {
        let input = SizedBatch {
            values: vec![],
            slots: vec![Ok(None), Err(failure)],
        };
        assert!(matches!(
            convert_history_value_reads(input, 2, limits()),
            Err(ConversionError::Unprojectable { reference_index: 1, failure: actual }) if actual == failure
        ));
    }
    Ok(())
}

#[test]
fn history_read_conversion_admits_actual_capacities_and_independent_limits()
-> Result<(), Box<dyn Error>> {
    let first = convert_history_value_reads(healthy(), 1, limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    first?;
    let mut leaf = Vec::with_capacity(8_192);
    leaf.push(b'x');
    assert!(leaf.capacity() >= 8_192);
    let mut tight = limits();
    tight.maximum_temporary_bytes = 4_096;
    let input = SizedBatch {
        values: vec![Value::Stored(leaf)],
        slots: vec![Ok(Some(0))],
    };
    assert!(matches!(
        convert_history_value_reads(input, 1, tight),
        Err(ConversionError::Limit)
    ));
    let mut values = Vec::with_capacity(512);
    values.push(Value::Stored(vec![b'x']));
    assert!(values.capacity() * std::mem::size_of::<Value>() > tight.maximum_temporary_bytes);
    let input = SizedBatch {
        values,
        slots: vec![Ok(Some(0))],
    };
    assert!(matches!(
        convert_history_value_reads(input, 1, tight),
        Err(ConversionError::Limit)
    ));
    let mut slots = Vec::with_capacity(8_192);
    slots.push(Ok(Some(0)));
    let input = SizedBatch {
        values: vec![Value::Stored(vec![b'x'])],
        slots,
    };
    assert!(matches!(
        convert_history_value_reads(input, 1, tight),
        Err(ConversionError::Limit)
    ));
    for axis in 0..3 {
        let mut low = limits();
        match axis {
            0 => low.maximum_references = 1,
            1 => low.maximum_work = 1,
            _ => low.maximum_temporary_bytes = 1,
        }
        let input = SizedBatch {
            values: vec![Value::Stored(vec![b'x'])],
            slots: vec![Ok(Some(0)), Ok(None)],
        };
        assert!(matches!(
            convert_history_value_reads(input, 2, low),
            Err(ConversionError::Limit)
        ));
        match axis {
            0 => low.maximum_references = 0,
            1 => low.maximum_work = 0,
            _ => low.maximum_temporary_bytes = 0,
        }
        assert!(matches!(
            convert_history_value_reads(healthy(), 1, low),
            Err(ConversionError::InvalidLimits)
        ));
    }
    let empty = convert_history_value_reads(
        SizedBatch {
            values: vec![],
            slots: vec![],
        },
        0,
        limits(),
    )?;
    assert!(matches!(empty, Batch::Values(values) if values.is_empty()));
    Ok(())
}

#[test]
fn history_read_conversion_projects_complete_owned_documents_per_occurrence()
-> Result<(), Box<dyn Error>> {
    let encoded = b"FSV2\x02\x83h\x02a\x07m\0\0\0\x01x".to_vec();
    let pointer = encoded.as_ptr();
    let input = SizedBatch {
        values: vec![Value::Stored(encoded)],
        slots: vec![Ok(Some(0))],
    };
    let converted = convert_history_value_reads(input, 1, limits());
    assert!(converted.is_ok(), "{:?}", converted.as_ref().err());
    let converted = converted?;
    let Batch::Values(values) = &converted else {
        return Err("expected owned values".into());
    };
    assert!(matches!(&values[0], Read::Stored(bytes) if bytes.as_ptr() == pointer));
    let history = [
        HistoryValueEntry {
            source: "event-one",
            references: [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil],
        },
        HistoryValueEntry {
            source: "event-two",
            references: [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil],
        },
    ];
    let plan = prepare_history_values(
        &history,
        HistoryValueReturn::Enabled { max_bytes: 100 },
        projection_limits(),
    )?;
    assert_eq!(plan.references(), [b"ref".as_slice()]);
    let overlay = apply_history_value_results(plan, Some(converted), &context())?;
    assert_eq!(overlay.history[0].source, "event-one");
    assert_eq!(overlay.history[1].source, "event-two");
    assert_eq!(
        overlay
            .writes
            .iter()
            .map(|w| (w.entry_index, w.field.key()))
            .collect::<Vec<_>>(),
        [
            (0, "payload"),
            (0, "payload_size"),
            (1, "payload"),
            (1, "payload_size")
        ]
    );
    let documents = overlay
        .writes
        .iter()
        .filter_map(|write| match &write.value {
            HistoryValue::Decoded(document) => Some(document),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 2);
    assert_ne!(
        documents[0].encoded().as_ptr(),
        documents[1].encoded().as_ptr()
    );
    for document in documents {
        assert_eq!(document.encoded(), b"FSV2\x02\x83h\x02a\x07m\0\0\0\x01x");
        let Some(Node::Tuple { children }) = document.root() else {
            return Err("expected tuple".into());
        };
        let elements = document.edge_range(children.clone()).ok_or("tuple edges")?;
        assert_eq!(elements.len(), 2);
        assert!(matches!(document.node(elements[0]), Some(Node::Integer(7))));
        assert!(
            matches!(document.node(elements[1]), Some(Node::Binary { bytes }) if document.byte_range(bytes.clone()) == Some(b"x"))
        );
    }
    Ok(())
}

#[test]
fn history_read_conversion_omission_skips_decoder_and_context() -> Result<(), Box<dyn Error>> {
    let input = SizedBatch {
        values: vec![Value::Omitted { stored_bytes: 100 }],
        slots: vec![Ok(Some(0))],
    };
    let converted = convert_history_value_reads(input, 1, limits());
    assert!(converted.is_ok(), "{:?}", converted.as_ref().err());
    let history = [HistoryValueEntry {
        source: "unchanged fields",
        references: [Ref::Binary(b"ref"), Ref::Nil, Ref::NonBinary],
    }];
    let mut profile = projection_limits();
    profile.decoder.max_work_units = 0;
    let plan = prepare_history_values(
        &history,
        HistoryValueReturn::Enabled { max_bytes: 10 },
        profile,
    )?;
    let overlay = apply_history_value_results(plan, Some(converted?), &context())?;
    assert_eq!(overlay.history[0].source, "unchanged fields");
    assert_eq!(overlay.writes.len(), 2);
    assert_eq!(overlay.writes[0].field.key(), "payload_omitted");
    assert!(matches!(
        overlay.writes[0].value,
        HistoryValue::Boolean(true)
    ));
    assert_eq!(overlay.writes[1].field.key(), "payload_size");
    assert!(matches!(
        overlay.writes[1].value,
        HistoryValue::StoredSize(100)
    ));
    Ok(())
}
