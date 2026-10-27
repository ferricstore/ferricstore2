use std::mem::size_of;

use super::fixtures as f;
use super::{
    HistoryValue as V, HistoryValueError as E, HistoryValueLimits, HistoryValueReadBatch as B,
    HistoryValueReadResult as R, HistoryValueReference as Ref, HistoryValueReturn as Return,
    HistoryValueWrite, OriginalFlowValueError as D, OriginalValueContext, OriginalValueLimits,
    TestResult, apply_history_value_results, decode_original_flow_value, prepare_history_values,
};

#[test]
fn history_values_checks_signed_maximum_thresholds_and_overflow() -> TestResult {
    let source = "signed cap";
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    for (cap, bytes, omitted, threshold) in [
        (i128::MIN, b"".as_slice(), true, 1),
        (-1, b"", true, 1),
        (0, b"", false, 1),
        (0, b"x", true, 1),
        (1, b"x", false, 2),
    ] {
        let prepared =
            prepare_history_values(&history, Return::Enabled { max_bytes: cap }, f::limits());
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let plan = prepared?;
        assert_eq!(plan.read_threshold(), Some(threshold));
        let applied = apply_history_value_results(
            plan,
            Some(B::Values(vec![R::Stored(bytes.to_vec())])),
            &f::context(),
        );
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        let overlay = applied?;
        let first = if omitted {
            "payload_omitted"
        } else {
            "payload"
        };
        f::assert_fields(&overlay, &[(0, first), (0, "payload_size")]);
        assert!(matches!(overlay.writes[1].value, V::StoredSize(size) if size == bytes.len()));
        if omitted {
            assert!(matches!(overlay.writes[0].value, V::Boolean(true)));
        } else if let V::Decoded(document) = &overlay.writes[0].value {
            f::assert_binary(document, bytes);
        } else {
            return Err("expected binary at nonnegative exact cap".into());
        }
    }
    let maximum = i128::try_from(usize::MAX)?;
    let exact = prepare_history_values(
        &history,
        Return::Enabled {
            max_bytes: maximum - 1,
        },
        f::limits(),
    );
    assert!(exact.is_ok(), "{:?}", exact.as_ref().err());
    let plan = exact?;
    assert_eq!(plan.read_threshold(), Some(usize::MAX));
    let applied =
        apply_history_value_results(plan, Some(B::Values(vec![R::Missing])), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    f::assert_fields(&applied?, &[(0, "payload"), (0, "payload_missing")]);
    for cap in [maximum, i128::MAX] {
        assert!(matches!(
            prepare_history_values(&history, Return::Enabled { max_bytes: cap }, f::limits()),
            Err(E::Limit)
        ));
    }
    Ok(())
}

#[test]
fn history_values_propagates_context_profile_and_decoder_resource_errors() -> TestResult {
    let source = "decoder errors keep their origin";
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let stored = b"FSV2\x02\x83w\x02ok";
    assert!(
        decode_original_flow_value(stored.to_vec(), &f::context(), f::limits().decoder).is_ok()
    );
    let prepared =
        prepare_history_values(&history, Return::Enabled { max_bytes: 1_024 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied = apply_history_value_results(
        prepared?,
        Some(B::Values(vec![R::Stored(stored.to_vec())])),
        &f::context(),
    );
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload"), (0, "payload_size")]);
    assert!(matches!(&overlay.writes[0].value, V::Decoded(document)
        if matches!(document.root(), Some(super::OriginalValueNode::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(b"ok".as_slice()))));
    let mut unavailable = f::context();
    unavailable.safe_atoms = None;
    assert_decode_refusal(stored, &unavailable, f::limits().decoder, 0)?;
    for bytes in [
        b"FSV2\x02\x83I\0\x01".as_slice(),
        b"FSV2\x02\x83K\0\0\x01",
        b"FSV2\x02\x83y\0\0\0\0j",
    ] {
        assert_decode_refusal(bytes, &f::context(), f::limits().decoder, 0)?;
    }
    assert_decode_refusal(
        b"FSV2\x02\x83M\0\0\0\0\0",
        &f::context(),
        f::limits().decoder,
        1,
    )?;
    let nested = b"FSV2\x02\x83h\x02h\x01a\x07a\x08";
    assert!(
        decode_original_flow_value(nested.to_vec(), &f::context(), f::limits().decoder).is_ok()
    );
    for axis in 0..5 {
        for (bound, expected) in [(0, 2), (1, 3)] {
            let mut decoder = f::limits().decoder;
            *match axis {
                0 => &mut decoder.max_encoded_bytes,
                1 => &mut decoder.max_nodes,
                2 => &mut decoder.max_edges,
                3 => &mut decoder.max_depth,
                _ => &mut decoder.max_work_units,
            } = bound;
            assert_decode_refusal(nested, &f::context(), decoder, expected)?;
        }
    }
    assert_closed_atom_is_corruption()?;
    // ReserveFailed is preserved by the selected API; no fabricated OOM here.
    Ok(())
}

#[test]
fn history_values_charges_invalid_decode_attempts_without_resetting_outer_work() -> TestResult {
    let source = "two invalid attempts followed by a healthy third";
    let history = [f::entry(
        &source,
        [Ref::Binary(b"a"), Ref::Binary(b"b"), Ref::Binary(b"c")],
    )];
    let limits = f::limits();
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, limits);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    let initial_work = plan.charged_work();
    let initial_requested = plan.charged_requested_bytes();
    let applied = apply_history_value_results(plan, Some(invalid_then_healthy()), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    assert_continuation(&overlay)?;
    assert!(overlay.charged_work <= limits.maximum_work_units);
    assert!(overlay.charged_requested_bytes <= limits.maximum_requested_bytes);
    assert!(overlay.charged_work >= initial_work + 3 * limits.decoder.max_work_units);
    assert!(
        overlay.charged_requested_bytes >= initial_requested + 3 * limits.decoder.max_work_units
    );
    let full = overlay.charged_work;
    // The fee of two invalid calls cannot disappear before the healthy third.
    let mut constrained = limits;
    constrained.maximum_work_units = 2 * limits.decoder.max_work_units - 1;
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, constrained);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    assert!(matches!(
        apply_history_value_results(prepared?, Some(invalid_then_healthy()), &f::context()),
        Err(E::Limit)
    ));
    for maximum in [full - 1, full] {
        let limited = HistoryValueLimits {
            maximum_work_units: maximum,
            ..limits
        };
        let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, limited);
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let applied =
            apply_history_value_results(prepared?, Some(invalid_then_healthy()), &f::context());
        if maximum == full {
            assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
            assert_continuation(&applied?)?;
        } else {
            assert!(matches!(applied, Err(E::Limit)));
        }
    }
    Ok(())
}

#[test]
fn history_values_admits_read_owners_patch_spines_and_per_occurrence_copies() -> TestResult {
    let source = "two occurrences, one incoming byte owner";
    let history = [f::entry(
        &source,
        [Ref::Binary(b"same"), Ref::Binary(b"same"), Ref::Nil],
    )];
    let limits = f::limits();
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 1 }, limits);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let plan = prepared?;
    assert_eq!(plan.references().len(), 1);
    let applied = apply_history_value_results(
        plan,
        Some(B::Values(vec![R::Stored(b"x".to_vec())])),
        &f::context(),
    );
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    assert_repeated_binary(&overlay)?;
    assert!(overlay.charged_work <= limits.maximum_work_units);
    assert!(overlay.charged_requested_bytes <= limits.maximum_requested_bytes);
    assert!(
        overlay.charged_requested_bytes
            >= 2 * limits.decoder.max_work_units + 4 * size_of::<HistoryValueWrite>()
    );
    let full = overlay.charged_requested_bytes;
    for maximum in [full - 1, full] {
        let bounded = HistoryValueLimits {
            maximum_requested_bytes: maximum,
            ..limits
        };
        let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 1 }, bounded);
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let applied = apply_history_value_results(
            prepared?,
            Some(B::Values(vec![R::Stored(b"x".to_vec())])),
            &f::context(),
        );
        if maximum == full {
            assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
            assert_repeated_binary(&applied?)?;
        } else {
            assert!(matches!(applied, Err(E::Limit)));
        }
    }
    assert_history_capacities(&history, limits, full)
}

fn assert_history_capacities(
    history: &[super::HistoryValueEntry<'_, &str>],
    limits: HistoryValueLimits,
    full: usize,
) -> TestResult {
    let mut spare = Vec::with_capacity(full.checked_add(1).ok_or("incoming capacity bound")?);
    spare.extend_from_slice(b"x");
    let bounded = HistoryValueLimits {
        maximum_requested_bytes: full,
        ..limits
    };
    assert!(spare.capacity() > bounded.maximum_requested_bytes);
    let prepared = prepare_history_values(history, Return::Enabled { max_bytes: 1 }, bounded);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    assert!(matches!(
        apply_history_value_results(
            prepared?,
            Some(B::Values(vec![R::Stored(spare)])),
            &f::context()
        ),
        Err(E::Limit)
    ));
    let mut values = Vec::with_capacity(full / size_of::<R<'_>>() + 1);
    values.push(R::Stored(b"x".to_vec()));
    let bounded = HistoryValueLimits {
        maximum_read_results: values.capacity(),
        maximum_requested_bytes: full,
        ..limits
    };
    assert!(values.capacity() * size_of::<R<'_>>() > bounded.maximum_requested_bytes);
    let prepared = prepare_history_values(history, Return::Enabled { max_bytes: 1 }, bounded);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    assert!(matches!(
        apply_history_value_results(prepared?, Some(B::Values(values)), &f::context()),
        Err(E::Limit)
    ));
    for maximum in [3, 4] {
        let bounded = HistoryValueLimits {
            maximum_writes: maximum,
            ..limits
        };
        let prepared = prepare_history_values(history, Return::Enabled { max_bytes: 1 }, bounded);
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let applied = apply_history_value_results(
            prepared?,
            Some(B::Values(vec![R::Stored(b"x".to_vec())])),
            &f::context(),
        );
        if maximum == 4 {
            assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
            assert_repeated_binary(&applied?)?;
        } else {
            assert!(matches!(applied, Err(E::Limit)));
        }
    }
    Ok(())
}

#[test]
fn history_values_refuses_malformed_batch_and_checked_resource_overflow() -> TestResult {
    let source = "batch shape is distinct from source corruption";
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 10 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied =
        apply_history_value_results(prepared?, Some(B::Values(vec![R::Missing])), &f::context());
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    f::assert_fields(&applied?, &[(0, "payload"), (0, "payload_missing")]);
    for batch in [
        None,
        Some(B::NonList),
        Some(B::Filtered {
            eligibility: Vec::new(),
            values: Vec::new(),
        }),
    ] {
        let prepared =
            prepare_history_values(&history, Return::Enabled { max_bytes: 10 }, f::limits());
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        assert!(matches!(
            apply_history_value_results(prepared?, batch, &f::context()),
            Err(E::InvalidReadBatch)
        ));
    }
    for mode in [Return::Disabled, Return::Enabled { max_bytes: 10 }] {
        let prepared = prepare_history_values(&history[..0], mode, f::limits());
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        assert!(matches!(
            apply_history_value_results(prepared?, Some(B::Values(Vec::new())), &f::context()),
            Err(E::InvalidReadBatch)
        ));
    }
    assert_invalid_profiles();
    assert_count_bounds()?;
    let enormous = HistoryValueLimits {
        maximum_work_units: usize::MAX,
        maximum_requested_bytes: usize::MAX,
        decoder: OriginalValueLimits {
            max_work_units: usize::MAX,
            ..f::limits().decoder
        },
        ..f::limits()
    };
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 10 }, enormous);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    assert!(matches!(
        apply_history_value_results(
            prepared?,
            Some(B::Values(vec![R::Stored(b"x".to_vec())])),
            &f::context()
        ),
        Err(E::Limit)
    ));
    Ok(())
}

fn assert_decode_refusal(
    stored: &[u8],
    context: &OriginalValueContext<'_>,
    decoder: OriginalValueLimits,
    expected: u8,
) -> TestResult {
    let old = decode_original_flow_value(stored.to_vec(), context, decoder);
    let Err(old) = old else {
        return Err("expected old decoder refusal premise".into());
    };
    assert_decoder_error(&old, expected, stored[6])?;
    let source = "operational source remains untouched";
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let limits = HistoryValueLimits {
        decoder,
        ..f::limits()
    };
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 1_024 }, limits);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied = apply_history_value_results(
        prepared?,
        Some(B::Values(vec![R::Stored(stored.to_vec())])),
        context,
    );
    let Err(E::Decode(error)) = applied else {
        return Err("decoder origin must remain an operational Decode error".into());
    };
    assert_decoder_error(&error, expected, stored[6])
}

fn assert_decoder_error(error: &D, expected: u8, tag: u8) -> TestResult {
    match (expected, error) {
        (
            0,
            D::UnsupportedContext {
                tag: actual,
                offset: 6,
            },
        )
        | (
            1,
            D::UnsupportedProfile {
                tag: actual,
                offset: 6,
            },
        ) => assert_eq!(*actual, tag),
        (2, D::InvalidLimits) | (3, D::Limit) => {}
        _ => return Err(format!("unexpected decoder origin {error:?}").into()),
    }
    Ok(())
}

fn assert_closed_atom_is_corruption() -> TestResult {
    let source = "closed absent atom";
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    let mut context = f::context();
    context.safe_atoms = Some(&[]);
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied = apply_history_value_results(
        prepared?,
        Some(B::Values(vec![R::Stored(b"FSV2\x02\x83w\x02ok".to_vec())])),
        &context,
    );
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload_error")]);
    assert!(matches!(
        overlay.writes[0].value,
        V::Error("ERR invalid flow value")
    ));
    Ok(())
}

fn invalid_then_healthy() -> B<'static> {
    B::Values(vec![
        R::Stored(b"FSV2\x02bad-one".to_vec()),
        R::Stored(b"FSV2\x02bad-two".to_vec()),
        R::Stored(b"good".to_vec()),
    ])
}

fn assert_continuation<T: ?Sized>(overlay: &super::HistoryValueOverlay<'_, T>) -> TestResult {
    f::assert_fields(
        overlay,
        &[
            (0, "payload_error"),
            (0, "result_error"),
            (0, "error"),
            (0, "error_size"),
        ],
    );
    for write in &overlay.writes[..2] {
        assert!(matches!(write.value, V::Error("ERR invalid flow value")));
    }
    if let V::Decoded(document) = &overlay.writes[2].value {
        f::assert_binary(document, b"good");
    } else {
        return Err("healthy third decode was lost".into());
    }
    assert!(matches!(overlay.writes[3].value, V::StoredSize(4)));
    Ok(())
}

fn assert_repeated_binary<T: ?Sized>(overlay: &super::HistoryValueOverlay<'_, T>) -> TestResult {
    f::assert_fields(
        overlay,
        &[
            (0, "payload"),
            (0, "payload_size"),
            (0, "result"),
            (0, "result_size"),
        ],
    );
    let mut first = None;
    for index in [0, 2] {
        let V::Decoded(document) = &overlay.writes[index].value else {
            return Err("expected separately owned binary".into());
        };
        f::assert_binary(document, b"x");
        assert_ne!(Some(document.encoded().as_ptr()), first);
        first = Some(document.encoded().as_ptr());
        assert!(matches!(overlay.writes[index + 1].value, V::StoredSize(1)));
    }
    Ok(())
}

fn assert_invalid_profiles() {
    let source = "positive profile controls";
    let history = [f::entry(&source, [Ref::Binary(b"ref"), Ref::Nil, Ref::Nil])];
    for axis in 0..6 {
        let mut limits = f::limits();
        *match axis {
            0 => &mut limits.maximum_entries,
            1 => &mut limits.maximum_references,
            2 => &mut limits.maximum_read_results,
            3 => &mut limits.maximum_writes,
            4 => &mut limits.maximum_work_units,
            _ => &mut limits.maximum_requested_bytes,
        } = 0;
        assert!(matches!(
            prepare_history_values(&history, Return::Enabled { max_bytes: 1 }, limits),
            Err(E::InvalidLimits)
        ));
    }
}

fn assert_count_bounds() -> TestResult {
    let sources = ["A", "B"];
    let history = [
        f::entry(&sources[0], [Ref::Binary(b"a"), Ref::Nil, Ref::Nil]),
        f::entry(&sources[1], [Ref::Binary(b"b"), Ref::Nil, Ref::Nil]),
    ];
    for bounded in [
        HistoryValueLimits {
            maximum_entries: 1,
            ..f::limits()
        },
        HistoryValueLimits {
            maximum_references: 1,
            ..f::limits()
        },
        HistoryValueLimits {
            maximum_work_units: 1,
            ..f::limits()
        },
        HistoryValueLimits {
            maximum_requested_bytes: 1,
            ..f::limits()
        },
    ] {
        assert!(matches!(
            prepare_history_values(&history, Return::Enabled { max_bytes: 1 }, bounded),
            Err(E::Limit)
        ));
    }
    let bounded = HistoryValueLimits {
        maximum_read_results: 1,
        ..f::limits()
    };
    let prepared = prepare_history_values(&history[..1], Return::Enabled { max_bytes: 1 }, bounded);
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    assert!(matches!(
        apply_history_value_results(
            prepared?,
            Some(B::Values(vec![R::Missing, R::Missing])),
            &f::context()
        ),
        Err(E::Limit)
    ));
    Ok(())
}
