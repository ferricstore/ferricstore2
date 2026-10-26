use std::collections::BTreeMap;

use super::fixtures as f;
use super::{
    HistoryValue as V, HistoryValueField as F, HistoryValueKind as K, HistoryValueReadBatch as B,
    HistoryValueReadResult as R, HistoryValueReference as Ref, HistoryValueReturn as Return,
    TestResult, apply_history_value_results, prepare_history_values,
};

#[test]
fn history_overlay_marks_storage_failures_without_exposing_their_reason() -> TestResult {
    // Original7486b505b2ba580e: arbitrary ref and the private diagnostic remain.
    let source = BTreeMap::from([("payload_ref", "ref")]);
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 10 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    assert_eq!(plan.references(), [b"ref".as_slice()]);
    let reason = (
        b"missing_file".as_slice(),
        b"/private/secret/path".as_slice(),
    );
    assert_eq!(reason.0, b"missing_file");
    let failure = R::StorageFailure {
        diagnostic: Some(reason.1),
    };
    let applied = apply_history_value_results(plan, Some(B::Values(vec![failure])), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload_error")]);
    assert!(matches!(
        overlay.writes[0].value,
        V::Error("ERR storage read failed")
    ));
    assert!(std::ptr::eq(
        overlay.history[0].source,
        std::ptr::from_ref(&source)
    ));
    assert_eq!(source.get("payload_ref"), Some(&"ref"));
    assert!(!source.contains_key("payload_error"));
    Ok(())
}

#[test]
fn history_overlay_file_reference_size_is_exact_stored_count() -> TestResult {
    // Originalbdafcf374348156d: /segment, offset10, STORED size100, cap10.
    let source = BTreeMap::from([("payload_ref", "ref")]);
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 10 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    assert_eq!(plan.read_threshold(), Some(11));
    let file_reference = (b"/segment".as_slice(), 10_usize, 100_usize);
    assert_eq!(
        (file_reference.0, file_reference.1),
        (b"/segment".as_slice(), 10)
    );
    let captured = R::StoredSize {
        bytes: file_reference.2,
    };
    let applied = apply_history_value_results(plan, Some(B::Values(vec![captured])), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload_omitted"), (0, "payload_size")]);
    assert!(matches!(overlay.writes[0].value, V::Boolean(true)));
    assert!(matches!(overlay.writes[1].value, V::StoredSize(100)));
    assert!(std::ptr::eq(
        overlay.history[0].source,
        std::ptr::from_ref(&source)
    ));
    assert!(!source.contains_key("payload"));
    Ok(())
}

#[test]
fn history_overlay_marks_corrupted_typed_values_without_inserting_payload() -> TestResult {
    // Original381f48c9d21d1d52: preserve both error equality AND absent payload.
    let source = BTreeMap::from([("payload_ref", "ref")]);
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let prepared =
        prepare_history_values(&history, Return::Enabled { max_bytes: 1_024 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    let corrupted = R::Stored(b"FSV2\x02not-an-external-term".to_vec());
    let applied =
        apply_history_value_results(plan, Some(B::Values(vec![corrupted])), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload_error")]);
    assert!(matches!(
        overlay.writes[0].value,
        V::Error("ERR invalid flow value")
    ));
    assert!(
        !overlay
            .writes
            .iter()
            .any(|write| write.field.key() == "payload")
    );
    assert!(!overlay.history[0].source.contains_key("payload"));
    Ok(())
}

#[test]
fn history_overlay_preserves_stale_fields_on_missing_omitted_and_unrelated_results() -> TestResult {
    let source = BTreeMap::from([
        ("payload", "old value"),
        ("payload_missing", "old missing"),
        ("payload_omitted", "old omitted"),
        ("payload_size", "old size"),
        ("payload_error", "old error"),
        ("unrelated", "retain me"),
    ]);
    let history = [
        f::entry(&source, [Ref::Binary(b"missing"), Ref::Nil, Ref::Nil]),
        f::entry(&source, [Ref::Binary(b"omitted"), Ref::Nil, Ref::Nil]),
        f::entry(&source, [Ref::Binary(b"unrelated"), Ref::Nil, Ref::Nil]),
    ];
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 2 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    let values = vec![R::Missing, R::Stored(b"three".to_vec()), R::Unrelated];
    let applied = apply_history_value_results(plan, Some(B::Values(values)), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(
        &overlay,
        &[
            (0, "payload"),
            (0, "payload_missing"),
            (1, "payload_omitted"),
            (1, "payload_size"),
        ],
    );
    assert!(matches!(overlay.writes[0].value, V::Nil));
    assert!(matches!(overlay.writes[1].value, V::Boolean(true)));
    assert!(matches!(overlay.writes[2].value, V::Boolean(true)));
    assert!(matches!(overlay.writes[3].value, V::StoredSize(5)));
    for original in overlay.history {
        assert!(std::ptr::eq(original.source, std::ptr::from_ref(&source)));
        assert_eq!(original.source.len(), 6);
        assert_eq!(original.source.get("payload"), Some(&"old value"));
        assert_eq!(original.source.get("payload_error"), Some(&"old error"));
        assert_eq!(original.source.get("payload_size"), Some(&"old size"));
        assert_eq!(original.source.get("payload_missing"), Some(&"old missing"));
        assert_eq!(original.source.get("payload_omitted"), Some(&"old omitted"));
        assert_eq!(original.source.get("unrelated"), Some(&"retain me"));
    }
    Ok(())
}

#[test]
fn history_overlay_applies_payload_result_error_writes_in_event_order() -> TestResult {
    let sources = ["event A", "event B"];
    let history = [
        f::entry(
            &sources[0],
            [Ref::Binary(b"a"), Ref::Binary(b"b"), Ref::Binary(b"c")],
        ),
        f::entry(
            &sources[1],
            [Ref::Binary(b"d"), Ref::Binary(b"e"), Ref::Binary(b"f")],
        ),
    ];
    let prepared =
        prepare_history_values(&history, Return::Enabled { max_bytes: 100 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    let values = vec![
        R::Missing,
        R::StorageFailure { diagnostic: None },
        R::StoredSize { bytes: 9 },
        R::Stored(b"live".to_vec()),
        R::Unrelated,
        R::Stored(b"FSV2\x02bad".to_vec()),
    ];
    let applied = apply_history_value_results(plan, Some(B::Values(values)), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(
        &overlay,
        &[
            (0, "payload"),
            (0, "payload_missing"),
            (0, "result_error"),
            (0, "error_omitted"),
            (0, "error_size"),
            (1, "payload"),
            (1, "payload_size"),
            (1, "error_error"),
        ],
    );
    assert!(matches!(overlay.writes[0].value, V::Nil));
    assert!(matches!(overlay.writes[1].value, V::Boolean(true)));
    assert!(matches!(
        overlay.writes[2].value,
        V::Error("ERR storage read failed")
    ));
    assert!(matches!(overlay.writes[3].value, V::Boolean(true)));
    assert!(matches!(overlay.writes[4].value, V::StoredSize(9)));
    if let V::Decoded(document) = &overlay.writes[5].value {
        f::assert_binary(document, b"live");
    } else {
        return Err("expected decoded payload".into());
    }
    assert!(matches!(overlay.writes[6].value, V::StoredSize(4)));
    assert!(matches!(
        overlay.writes[7].value,
        V::Error("ERR invalid flow value")
    ));
    assert_field_names();
    Ok(())
}

fn assert_field_names() {
    for (kind, expected) in [
        (
            K::Payload,
            [
                "payload",
                "payload_missing",
                "payload_omitted",
                "payload_size",
                "payload_error",
            ],
        ),
        (
            K::Result,
            [
                "result",
                "result_missing",
                "result_omitted",
                "result_size",
                "result_error",
            ],
        ),
        (
            K::Error,
            [
                "error",
                "error_missing",
                "error_omitted",
                "error_size",
                "error_error",
            ],
        ),
    ] {
        let fields = [
            F::Value(kind),
            F::Missing(kind),
            F::Omitted(kind),
            F::Size(kind),
            F::Error(kind),
        ];
        for (field, name) in fields.into_iter().zip(expected) {
            assert_eq!(field.key(), name);
        }
    }
}
