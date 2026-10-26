use super::fixtures as f;
use super::{
    HistoryValue as V, HistoryValueOverlay, HistoryValueReadBatch as B,
    HistoryValueReadResult as R, HistoryValueReference as Ref, HistoryValueReturn as Return,
    OriginalValueContext, TestResult, apply_history_value_results, prepare_history_values,
};

#[test]
fn history_values_collects_first_binary_references_before_size_filtering() -> TestResult {
    let sources = ["first", "second", "third"];
    let first = b"arbitrary-ref".to_vec();
    let equal = b"arbitrary-ref".to_vec();
    assert_ne!(first.as_ptr(), equal.as_ptr());
    let boundary = vec![b'x'; 65_535];
    let oversized = vec![b'y'; 65_536];
    let history = [
        f::entry(
            &sources[0],
            [Ref::Binary(&oversized), Ref::Binary(&first), Ref::Nil],
        ),
        f::entry(
            &sources[1],
            [
                Ref::Binary(&equal),
                Ref::Binary(&boundary),
                Ref::Binary(b""),
            ],
        ),
        f::entry(
            &sources[2],
            [Ref::NonBinary, Ref::Binary(b"last"), Ref::Nil],
        ),
    ];
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 8 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    assert_eq!(
        plan.references(),
        [first.as_slice(), boundary.as_slice(), b"last"]
    );
    assert_eq!(plan.references()[0].as_ptr(), first.as_ptr());
    assert_ne!(plan.references()[0].as_ptr(), equal.as_ptr());
    assert_eq!(plan.references()[1].len(), 65_535);
    assert_eq!(plan.references()[1].as_ptr(), boundary.as_ptr());
    assert_eq!(plan.read_threshold(), Some(9));
    assert!(std::ptr::eq(plan.history(), history.as_slice()));
    let values = vec![R::Unrelated, R::Unrelated, R::Unrelated];
    let applied = apply_history_value_results(plan, Some(B::Values(values)), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload_error")]);
    assert!(matches!(
        overlay.writes[0].value,
        V::Error("ERR payload_ref key too large")
    ));
    assert_eq!(overlay.history.len(), 3);
    Ok(())
}

#[test]
fn history_values_disabled_and_empty_requests_need_no_read_or_context() -> TestResult {
    let empty_references: &[&[u8]] = &[];
    let source = ("event", b"original fields".as_slice());
    let history = [f::entry(
        &source,
        [Ref::Binary(b"ref"), Ref::Nil, Ref::NonBinary],
    )];
    let missing_context = OriginalValueContext {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    };
    for empty in [false, true] {
        let selected = if empty { &history[..0] } else { &history[..] };
        let value_return = if empty {
            Return::Enabled {
                max_bytes: i128::MAX,
            }
        } else {
            Return::Disabled
        };
        let mut limits = f::limits();
        limits.decoder.max_encoded_bytes = 0;
        limits.decoder.max_work_units = 0;
        let prepared = prepare_history_values(selected, value_return, limits);
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let plan = prepared?;
        assert_eq!(plan.references(), empty_references);
        assert_eq!(plan.read_threshold(), None);
        assert_eq!(plan.history().len(), selected.len());
        assert!(matches!(plan.value_return(), Return::Disabled) || empty);
        let applied = apply_history_value_results(plan, None, &missing_context);
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        let overlay = applied?;
        assert!(overlay.writes.is_empty());
        assert!(std::ptr::eq(overlay.history, selected));
    }
    assert_eq!(source.0, "event");
    assert_eq!(source.1, b"original fields");
    Ok(())
}

#[test]
fn history_values_oversized_only_history_still_emits_kind_errors() -> TestResult {
    let empty_references: &[&[u8]] = &[];
    let source = ["payload remains", "result remains", "error remains"];
    let oversized = vec![0; 65_536];
    let history = [f::entry(
        &source,
        [
            Ref::Binary(&oversized),
            Ref::Binary(&oversized),
            Ref::Binary(&oversized),
        ],
    )];
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 1 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    assert_eq!(plan.references(), empty_references);
    assert_eq!(plan.read_threshold(), Some(2));
    let applied = apply_history_value_results(plan, None, &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(
        &overlay,
        &[
            (0, "payload_error"),
            (0, "result_error"),
            (0, "error_error"),
        ],
    );
    for (write, expected) in overlay.writes.iter().zip([
        "ERR payload_ref key too large",
        "ERR result_ref key too large",
        "ERR error_ref key too large",
    ]) {
        assert!(matches!(&write.value, V::Error(actual) if *actual == expected));
    }
    assert!(std::ptr::eq(
        overlay.history[0].source,
        std::ptr::from_ref(&source)
    ));
    assert_eq!(
        source,
        ["payload remains", "result remains", "error remains"]
    );
    let ignored = [f::entry(
        &source,
        [Ref::Nil, Ref::NonBinary, Ref::Binary(b"")],
    )];
    let prepared = prepare_history_values(&ignored, Return::Enabled { max_bytes: 0 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    assert_eq!(plan.references(), empty_references);
    assert_eq!(plan.read_threshold(), Some(1));
    let unavailable = OriginalValueContext {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    };
    let applied = apply_history_value_results(plan, None, &unavailable);
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    f::assert_fields(&applied?, &[]);
    for nonempty in [history.as_slice(), ignored.as_slice()] {
        let maximum = i128::try_from(usize::MAX)?;
        assert!(matches!(
            prepare_history_values(
                nonempty,
                Return::Enabled { max_bytes: maximum },
                f::limits()
            ),
            Err(super::HistoryValueError::Limit)
        ));
    }
    Ok(())
}

#[test]
fn history_values_preserves_source_zip_and_filtered_cardinality_branches() -> TestResult {
    let sources = ["event A", "event B"];
    let history = [
        f::entry(&sources[0], [Ref::Binary(b"a"), Ref::Nil, Ref::Nil]),
        f::entry(&sources[1], [Ref::Binary(b"b"), Ref::Nil, Ref::Nil]),
    ];
    // None denotes the captured post-ValueStore Values branch.
    for (mask, count, expected) in [
        (None, 2, [Some(b"A".as_slice()), Some(b"B".as_slice())]),
        (None, 1, [Some(b"A"), None]),
        (None, 3, [Some(b"A"), Some(b"B")]),
        (Some([true, true]), 1, [Some(b"A"), None]),
        (Some([true, true]), 3, [Some(b"A"), Some(b"B")]),
        (Some([true, false]), 1, [Some(b"A"), None]),
        (Some([true, false]), 0, [None, None]),
        (Some([true, false]), 2, [None, None]),
        (Some([false, true]), 1, [None, Some(b"A")]),
        (Some([false, true]), 0, [None, None]),
        (Some([false, true]), 2, [None, None]),
        (Some([false, false]), 0, [None, None]),
        (Some([false, false]), 1, [None, None]),
    ] {
        let prepared =
            prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, f::limits());
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let plan = prepared?;
        assert_eq!(plan.references(), [b"a".as_slice(), b"b"]);
        // A zipped-away native-context value must not be decoded as a third ref.
        let values = [b"A".as_slice(), b"B", b"FSV2\x02\x83I\0\x01"]
            .into_iter()
            .take(count)
            .map(|bytes| R::Stored(bytes.to_vec()))
            .collect();
        let batch = match mask {
            None => B::Values(values),
            Some(mask) => B::Filtered {
                eligibility: mask.to_vec(),
                values,
            },
        };
        let applied = apply_history_value_results(plan, Some(batch), &f::context());
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        assert_slots(&applied?, expected)?;
    }
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let mismatch = B::Filtered {
        eligibility: vec![false, true],
        values: vec![
            R::Stored(b"FSV2\x02\x83I\0\x01".to_vec()),
            R::Stored(b"FSV2\x02\x83I\0\x01".to_vec()),
        ],
    };
    let applied = apply_history_value_results(prepared?, Some(mismatch), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    assert_slots(&applied?, [None, None])?;
    Ok(())
}

fn assert_slots<T: ?Sized>(
    overlay: &HistoryValueOverlay<'_, T>,
    expected: [Option<&[u8]>; 2],
) -> TestResult {
    assert_eq!(overlay.history.len(), 2);
    assert_eq!(overlay.writes.len(), 4);
    for (index, expected) in expected.into_iter().enumerate() {
        let pair = &overlay.writes[index * 2..index * 2 + 2];
        assert_eq!(pair[0].entry_index, index);
        assert_eq!(pair[1].entry_index, index);
        assert_eq!(pair[0].field.key(), "payload");
        if let Some(bytes) = expected {
            assert_eq!(pair[1].field.key(), "payload_size");
            assert!(matches!(pair[1].value, V::StoredSize(1)));
            if let V::Decoded(document) = &pair[0].value {
                f::assert_binary(document, bytes);
            } else {
                return Err("expected decoded restored slot".into());
            }
        } else {
            assert_eq!(pair[1].field.key(), "payload_missing");
            assert!(matches!(pair[0].value, V::Nil));
            assert!(matches!(pair[1].value, V::Boolean(true)));
        }
    }
    Ok(())
}
