use super::fixtures as f;
use super::{
    HistoryValue as V, HistoryValueReadBatch as B, HistoryValueReadResult as R,
    HistoryValueReference as Ref, HistoryValueReturn as Return, OriginalValueContext,
    OriginalValueDocument, OriginalValueNode as N, TestResult, apply_history_value_results,
    decode_original_flow_value, prepare_history_values,
};

#[test]
fn history_values_decodes_common_raw_escape_and_empty_values_at_exact_caps() -> TestResult {
    let source = ("event", "original opaque fields");
    let history = [f::entry(&source, [Ref::Binary(b"raw"), Ref::Nil, Ref::Nil])];
    for (stored, expected) in [
        (b"plain".as_slice(), b"plain".as_slice()),
        (b"FSV2", b"FSV2"),
        (b"FSV2\x03opaque", b"FSV2\x03opaque"),
        (b"FSV2\x01FSV2\x02opaque", b"FSV2\x02opaque"),
        (b"FSV2\x01", b""),
        (b"", b""),
    ] {
        let old = decode_original_flow_value(stored.to_vec(), &f::context(), f::limits().decoder);
        assert!(old.is_ok(), "{:?}", old.as_ref().err());
        if let Ok(document) = old {
            f::assert_binary(&document, expected);
        }
        let maximum = i128::try_from(stored.len())?;
        let prepared = prepare_history_values(
            &history,
            Return::Enabled { max_bytes: maximum },
            f::limits(),
        );
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let plan = prepared?;
        assert_eq!(
            plan.read_threshold(),
            Some(stored.len().max(1) + usize::from(!stored.is_empty()))
        );
        let applied = apply_history_value_results(
            plan,
            Some(B::Values(vec![R::Stored(stored.to_vec())])),
            &f::context(),
        );
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        let overlay = applied?;
        f::assert_fields(&overlay, &[(0, "payload"), (0, "payload_size")]);
        assert!(matches!(overlay.writes[1].value, V::StoredSize(size) if size == stored.len()));
        if let V::Decoded(document) = &overlay.writes[0].value {
            assert_eq!(document.encoded(), stored);
            f::assert_binary(document, expected);
        } else {
            return Err("expected complete raw/escaped binary document".into());
        }
    }
    Ok(())
}

#[test]
fn history_values_projects_complete_compound_at_184_and_omits_at_183() -> TestResult {
    let stored = f::compound184();
    assert_eq!(stored.len(), 184);
    let old = decode_original_flow_value(f::compound184(), &f::context(), f::limits().decoder);
    assert!(old.is_ok(), "{:?}", old.as_ref().err());
    if let Ok(document) = old {
        f::assert_compound(&document)?;
    }
    let source = ["all existing fields remain borrowed"];
    let history = [f::entry(
        &source,
        [Ref::Binary(b"compound"), Ref::Nil, Ref::Nil],
    )];
    for cap in [184, 183] {
        let mut limits = f::limits();
        let mut context = f::context();
        if cap == 183 {
            limits.decoder.max_encoded_bytes = 0;
            limits.decoder.max_work_units = 0;
            context.safe_atoms = None;
        }
        let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: cap }, limits);
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let applied = apply_history_value_results(
            prepared?,
            Some(B::Values(vec![R::Stored(f::compound184())])),
            &context,
        );
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        let overlay = applied?;
        if cap == 184 {
            f::assert_fields(&overlay, &[(0, "payload"), (0, "payload_size")]);
            if let V::Decoded(document) = &overlay.writes[0].value {
                assert_eq!(document.encoded(), stored);
                f::assert_compound(document)?;
            } else {
                return Err("expected full typed compound".into());
            }
        } else {
            f::assert_fields(&overlay, &[(0, "payload_omitted"), (0, "payload_size")]);
            assert!(matches!(overlay.writes[0].value, V::Boolean(true)));
        }
        assert!(matches!(overlay.writes[1].value, V::StoredSize(184)));
        assert!(std::ptr::eq(
            overlay.history[0].source,
            std::ptr::from_ref(&source)
        ));
    }
    Ok(())
}

#[test]
fn history_values_keeps_typed_key_aliases_and_foreign_identity_data() -> TestResult {
    let source = "exact map keys";
    let history = [f::entry(&source, [Ref::Binary(b"map"), Ref::Nil, Ref::Nil])];
    let stored = distinct_key_map();
    let old = decode_original_flow_value(distinct_key_map(), &f::context(), f::limits().decoder);
    assert!(old.is_ok(), "{:?}", old.as_ref().err());
    if let Ok(document) = old {
        assert_distinct_keys(&document)?;
    }
    let prepared =
        prepare_history_values(&history, Return::Enabled { max_bytes: 1_024 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied = apply_history_value_results(
        prepared?,
        Some(B::Values(vec![R::Stored(stored)])),
        &f::context(),
    );
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload"), (0, "payload_size")]);
    if let V::Decoded(document) = &overlay.writes[0].value {
        assert_distinct_keys(document)?;
    } else {
        return Err("expected distinct exact key map".into());
    }
    for (left, right) in [
        (b"a\x01".as_slice(), b"b\0\0\0\x01".as_slice()),
        (b"a\x01", b"n\x02\0\x01\0"),
        (b"k\0\x01a", b"l\0\0\0\x01aaj"),
        (b"w\x03nil", b"d\0\x03nil"),
    ] {
        assert_alias_error(left, right)?;
    }
    assert_foreign_occurrences()?;
    Ok(())
}

#[test]
fn history_values_decodes_each_occurrence_into_distinct_owned_documents() -> TestResult {
    let original = decode_original_flow_value(
        b"real original source".to_vec(),
        &f::context(),
        f::limits().decoder,
    );
    assert!(original.is_ok(), "{:?}", original.as_ref().err());
    let original = original.map_err(|error| format!("old source premise: {error:?}"))?;
    let reference = b"repeat".to_vec();
    let history = [
        f::entry(&original, [Ref::Binary(&reference), Ref::Nil, Ref::Nil]),
        f::entry(&original, [Ref::Nil, Ref::Binary(&reference), Ref::Nil]),
    ];
    let stored = b"FSV2\x02\x83h\x02w\x02okm\0\0\0\x02\xff\0";
    let (overlay, read_pointer) = {
        let atom_owner = [String::from("ok")];
        let names = [atom_owner[0].as_str()];
        let context = OriginalValueContext {
            safe_atoms: Some(&names),
            active_exports: Some(&[]),
            local_node: None,
            local_functions: Some(&[]),
        };
        let prepared =
            prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, f::limits());
        assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
        let plan = prepared?;
        assert_eq!(plan.references(), [b"repeat".as_slice()]);
        let read = stored.to_vec();
        let pointer = read.as_ptr();
        let applied =
            apply_history_value_results(plan, Some(B::Values(vec![R::Stored(read)])), &context);
        assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
        (applied?, pointer)
    };
    // Incoming read, context descriptors and atom-owner String have all dropped.
    f::assert_fields(
        &overlay,
        &[
            (0, "payload"),
            (0, "payload_size"),
            (1, "result"),
            (1, "result_size"),
        ],
    );
    let mut previous = None;
    for index in [0, 2] {
        let V::Decoded(document) = &overlay.writes[index].value else {
            return Err("expected independently owned document".into());
        };
        assert_eq!(document.encoded(), stored);
        assert_ne!(document.encoded().as_ptr(), read_pointer);
        assert_ne!(Some(document.encoded().as_ptr()), previous);
        previous = Some(document.encoded().as_ptr());
        assert_tuple(document)?;
        assert!(
            matches!(overlay.writes[index + 1].value, V::StoredSize(size) if size == stored.len())
        );
    }
    for entry in overlay.history {
        assert!(std::ptr::eq(entry.source, std::ptr::from_ref(&original)));
        f::assert_binary(entry.source, b"real original source");
    }
    Ok(())
}

fn distinct_key_map() -> Vec<u8> {
    let mut stored = b"FSV2\x02\x83t\0\0\0\x04a\x01a\x0b".to_vec();
    for (bits, value) in [
        (0x3ff0_0000_0000_0000_u64, 12),
        (0, 13),
        (0x8000_0000_0000_0000, 14),
    ] {
        stored.push(70);
        stored.extend_from_slice(&bits.to_be_bytes());
        stored.extend_from_slice(&[97, value]);
    }
    stored
}

fn assert_distinct_keys(document: &OriginalValueDocument) -> TestResult {
    let Some(N::Map { entries }) = document.root() else {
        return Err("expected exact-key map".into());
    };
    let ids = document.edge_range(entries.clone()).ok_or("map edges")?;
    assert_eq!(ids.len(), 8);
    let mut seen = 0;
    for pair in ids.as_chunks::<2>().0 {
        let bit = match (document.node(pair[0]), document.node(pair[1])) {
            (Some(N::Integer(1)), Some(N::Integer(11))) => 1,
            (Some(N::FloatBits(0x3ff0_0000_0000_0000)), Some(N::Integer(12))) => 2,
            (Some(N::FloatBits(0)), Some(N::Integer(13))) => 4,
            (Some(N::FloatBits(0x8000_0000_0000_0000)), Some(N::Integer(14))) => 8,
            _ => return Err("unexpected exact map key/value".into()),
        };
        assert_eq!(seen & bit, 0);
        seen |= bit;
    }
    assert_eq!(seen, 15);
    Ok(())
}

fn assert_alias_error(left: &[u8], right: &[u8]) -> TestResult {
    for key in [left, right] {
        let mut single = b"FSV2\x02\x83t\0\0\0\x01".to_vec();
        single.extend_from_slice(key);
        single.push(106);
        assert!(decode_original_flow_value(single, &f::context(), f::limits().decoder).is_ok());
    }
    let mut stored = b"FSV2\x02\x83t\0\0\0\x02".to_vec();
    stored.extend_from_slice(left);
    stored.push(106);
    stored.extend_from_slice(right);
    stored.push(106);
    let source = "duplicate source key";
    let history = [f::entry(
        &source,
        [Ref::Binary(b"alias"), Ref::Nil, Ref::Nil],
    )];
    let prepared =
        prepare_history_values(&history, Return::Enabled { max_bytes: 1_024 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied = apply_history_value_results(
        prepared?,
        Some(B::Values(vec![R::Stored(stored)])),
        &f::context(),
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

fn assert_tuple(document: &OriginalValueDocument) -> TestResult {
    let Some(N::Tuple { children }) = document.root() else {
        return Err("expected owned tuple".into());
    };
    let ids = document.edge_range(children.clone()).ok_or("tuple edges")?;
    assert_eq!(ids.len(), 2);
    assert!(matches!(document.node(ids[0]), Some(N::Atom { bytes, .. })
        if document.byte_range(bytes.clone()) == Some(b"ok".as_slice())));
    assert!(matches!(document.node(ids[1]), Some(N::Binary { bytes })
        if document.byte_range(bytes.clone()) == Some(b"\xff\0".as_slice())));
    Ok(())
}

fn assert_foreign_occurrences() -> TestResult {
    let source = "foreign identity is data";
    let history = [f::entry(source, [Ref::Binary(b"pid"), Ref::Nil, Ref::Nil])];
    let stored = b"FSV2\x02\x83Xw\x0dnonode@nohost\0\0\0\x5e\0\0\0\0\0\0\0\0";
    let old = decode_original_flow_value(stored.to_vec(), &f::context(), f::limits().decoder);
    assert!(old.is_ok(), "{:?}", old.as_ref().err());
    let prepared = prepare_history_values(&history, Return::Enabled { max_bytes: 64 }, f::limits());
    assert!(prepared.is_ok(), "{:?}", prepared.as_ref().err());
    let applied = apply_history_value_results(
        prepared?,
        Some(B::Values(vec![R::Stored(stored.to_vec())])),
        &f::context(),
    );
    assert!(applied.is_ok(), "{:?}", applied.as_ref().err());
    let overlay = applied?;
    f::assert_fields(&overlay, &[(0, "payload"), (0, "payload_size")]);
    let V::Decoded(document) = &overlay.writes[0].value else {
        return Err("expected typed PID identity".into());
    };
    assert_eq!(document.encoded(), stored);
    assert!(matches!(overlay.writes[1].value, V::StoredSize(size) if size == stored.len()));
    let Some(N::ForeignPid {
        node,
        id,
        serial,
        creation,
        local_to_context,
    }) = document.root()
    else {
        return Err("PID must remain a distinct typed identity".into());
    };
    assert_eq!(
        (*id, *serial, *creation, *local_to_context),
        (94, 0, 0, true)
    );
    assert!(matches!(document.node(*node), Some(N::Atom { bytes, .. })
        if document.byte_range(bytes.clone()) == Some(b"nonode@nohost".as_slice())));
    Ok(())
}
