use super::{C, D, E, L, N, decode};

#[test]
fn flow_value_decodes_portable_etf_roots() {
    let context = C {
        safe_atoms: Some(&[
            "atom", "ok", "binary", "bits", "float", "improper", "tail", "integer", "nested",
            "true", "nil", "tuple",
        ]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: Some(&[]),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    // Complete independently captured184-byte source compound, unchanged.
    let encoded = [
        0x46, 0x53, 0x56, 0x32, 0x02, 0x83, 0x74, 0x00, 0x00, 0x00, 0x08, 0x77, 0x04, 0x61, 0x74,
        0x6f, 0x6d, 0x77, 0x02, 0x6f, 0x6b, 0x77, 0x06, 0x62, 0x69, 0x6e, 0x61, 0x72, 0x79, 0x6d,
        0x00, 0x00, 0x00, 0x02, 0xff, 0x00, 0x77, 0x04, 0x62, 0x69, 0x74, 0x73, 0x4d, 0x00, 0x00,
        0x00, 0x01, 0x03, 0xa0, 0x77, 0x05, 0x66, 0x6c, 0x6f, 0x61, 0x74, 0x46, 0x80, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x77, 0x08, 0x69, 0x6d, 0x70, 0x72, 0x6f, 0x70, 0x65, 0x72,
        0x6c, 0x00, 0x00, 0x00, 0x01, 0x61, 0x01, 0x77, 0x04, 0x74, 0x61, 0x69, 0x6c, 0x77, 0x07,
        0x69, 0x6e, 0x74, 0x65, 0x67, 0x65, 0x72, 0x6e, 0x1a, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x77, 0x06, 0x6e, 0x65, 0x73, 0x74, 0x65, 0x64, 0x74,
        0x00, 0x00, 0x00, 0x01, 0x6d, 0x00, 0x00, 0x00, 0x01, 0x6b, 0x6c, 0x00, 0x00, 0x00, 0x03,
        0x61, 0x01, 0x77, 0x04, 0x74, 0x72, 0x75, 0x65, 0x77, 0x03, 0x6e, 0x69, 0x6c, 0x6a, 0x77,
        0x05, 0x74, 0x75, 0x70, 0x6c, 0x65, 0x68, 0x02, 0x6d, 0x00, 0x00, 0x00, 0x01, 0x78, 0x62,
        0xff, 0xff, 0xff, 0xf7,
    ];
    assert_eq!(encoded.len(), 184);
    let actual = decode(encoded.to_vec(), &context, limits);
    assert!(actual.is_ok(), "{actual:?}");
    if let Ok(document) = actual {
        assert_eq!(document.encoded(), encoded);
        assert_compound(&document);
    }
    for (stored, kind) in [
        (b"FSV2\x02\x83w\x03nil".as_slice(), 0),
        (b"FSV2\x02\x83j", 1),
        (b"FSV2\x02\x83h\0", 2),
        (b"FSV2\x02\x83k\0\x01a", 3),
    ] {
        let actual = decode(stored.to_vec(), &context, limits);
        assert!(actual.is_ok(), "{stored:?}: {actual:?}");
        if let Ok(document) = actual {
            let valid = match (kind, document.root()) {
                (0, Some(N::Atom { bytes, .. })) => document.byte_range(bytes.clone()) == Some(b"nil".as_slice()),
                (1, Some(N::EmptyList)) => true,
                (2, Some(N::Tuple { children })) => document.edge_range(children.clone()) == Some([].as_slice()),
                (3, Some(N::List { heads, tail })) => document.edge_range(heads.clone()).is_some_and(|ids| matches!(ids, [one] if matches!(document.node(*one), Some(N::Integer(97))))) && matches!(document.node(*tail), Some(N::EmptyList)),
                _ => false,
            };
            assert!(valid, "{document:?}");
        }
    }
}

#[test]
fn flow_value_rejects_duplicate_exact_keys_across_encodings() {
    let context = C {
        safe_atoms: Some(&["nil", "é"]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: Some(&[]),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    // Each independently valid pair denotes one exact source map key.
    for (left, right) in [
        (b"a\x01".as_slice(), b"b\0\0\0\x01".as_slice()),
        (b"a\x01", b"n\x02\0\x01\0"),
        (b"a\0", b"n\x02\xff\0\0"),
        (b"k\0\x01a", b"l\0\0\0\x01aaj"),
        (b"w\x03nil", b"d\0\x03nil"),
        (b"w\x02\xc3\xa9", b"d\0\x01\xe9"),
        (b"h\x01a\x01", b"h\x01n\x01\0\x01"),
        (b"a\x07", b"l\0\0\0\0a\x07"),
        (
            b"l\0\0\0\x02a\x01a\x02j",
            b"l\0\0\0\x01a\x01l\0\0\0\x01a\x02j",
        ),
        (
            b"t\0\0\0\x02m\0\0\0\x01aa\x01m\0\0\0\x01ba\x02",
            b"t\0\0\0\x02m\0\0\0\x01ba\x02m\0\0\0\x01aa\x01",
        ),
    ] {
        for key in [left, right] {
            let mut single = b"FSV2\x02\x83t\0\0\0\x01".to_vec();
            single.extend_from_slice(key);
            single.push(106);
            assert!(decode(single, &context, limits).is_ok(), "key {key:?}");
        }
        for (first, second) in [(left, right), (right, left)] {
            let mut stored = b"FSV2\x02\x83t\0\0\0\x02".to_vec();
            stored.extend_from_slice(first);
            stored.push(106);
            stored.extend_from_slice(second);
            stored.push(106);
            assert!(matches!(
                decode(stored, &context, limits),
                Err(E::InvalidFlowValue)
            ));
        }
    }
}

#[test]
fn flow_value_preserves_distinct_zero_and_integer_float_keys() {
    let context = C {
        safe_atoms: Some(&[]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: Some(&[]),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
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
    let actual = decode(stored, &context, limits);
    assert!(actual.is_ok(), "{actual:?}");
    if let Ok(document) = actual {
        let mut seen = 0_u8;
        if let Some(N::Map { entries }) = document.root()
            && let Some(ids) = document.edge_range(entries.clone())
        {
            assert_eq!(ids.len(), 8);
            for pair in ids.as_chunks::<2>().0 {
                let bit = match (document.node(pair[0]), document.node(pair[1])) {
                    (Some(N::Integer(1)), Some(N::Integer(11))) => 1,
                    (Some(N::FloatBits(0x3ff0_0000_0000_0000)), Some(N::Integer(12))) => 2,
                    (Some(N::FloatBits(0)), Some(N::Integer(13))) => 4,
                    (Some(N::FloatBits(0x8000_0000_0000_0000)), Some(N::Integer(14))) => 8,
                    _ => 0,
                };
                assert_ne!(bit, 0, "{pair:?}: {document:?}");
                seen |= bit;
            }
        }
        assert_eq!(seen, 15);
    }
}

fn assert_compound(document: &D) {
    assert!(matches!(document.root(), Some(N::Map { entries })
        if document.edge_range(entries.clone()).is_some_and(|ids| ids.len() == 16)));
    if let Some(N::Map { entries }) = document.root()
        && let Some(ids) = document.edge_range(entries.clone())
    {
        let keys = [
            b"atom".as_slice(),
            b"binary",
            b"bits",
            b"float",
            b"improper",
            b"integer",
            b"nested",
            b"tuple",
        ];
        for (pair, key) in ids.as_chunks::<2>().0.iter().zip(keys) {
            assert!(matches!(document.node(pair[0]), Some(N::Atom { bytes, .. })
                if document.byte_range(bytes.clone()) == Some(key)));
        }
        assert!(matches!(document.node(ids[1]), Some(N::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(b"ok".as_slice())));
        assert!(matches!(document.node(ids[3]), Some(N::Binary { bytes })
            if document.byte_range(bytes.clone()) == Some(b"\xff\0".as_slice())));
        assert!(
            matches!(document.node(ids[5]), Some(N::BitString { bytes, last_bits: 3 })
            if document.byte_range(bytes.clone()) == Some(b"\xa0".as_slice()))
        );
        assert!(matches!(
            document.node(ids[7]),
            Some(N::FloatBits(0x8000_0000_0000_0000))
        ));
        assert!(
            matches!(document.node(ids[9]), Some(N::List { heads, tail })
            if document.edge_range(heads.clone()).is_some_and(|heads| matches!(heads, [one]
                if matches!(document.node(*one), Some(N::Integer(1)))))
                && matches!(document.node(*tail), Some(N::Atom { bytes, .. })
                    if document.byte_range(bytes.clone()) == Some(b"tail".as_slice())))
        );
        let mut magnitude = [0; 26];
        magnitude[0] = 5;
        magnitude[25] = 1;
        assert!(
            matches!(document.node(ids[11]), Some(N::BigInteger { negative: false, digits })
            if document.byte_range(digits.clone()) == Some(magnitude.as_slice()))
        );
        assert!(matches!(document.node(ids[13]), Some(N::Map { entries })
            if document.edge_range(entries.clone()).is_some_and(|pairs| matches!(pairs, [key, value]
                if matches!(document.node(*key), Some(N::Binary { bytes })
                    if document.byte_range(bytes.clone()) == Some(b"k".as_slice()))
                    && matches!(document.node(*value), Some(N::List { heads, tail })
                        if matches!(document.node(*tail), Some(N::EmptyList))
                            && document.edge_range(heads.clone()).is_some_and(|heads| matches!(heads, [one, truth, nil]
                                if matches!(document.node(*one), Some(N::Integer(1)))
                                    && matches!(document.node(*truth), Some(N::Atom { bytes, .. })
                                        if document.byte_range(bytes.clone()) == Some(b"true".as_slice()))
                                    && matches!(document.node(*nil), Some(N::Atom { bytes, .. })
                                        if document.byte_range(bytes.clone()) == Some(b"nil".as_slice())))))))));
        assert!(matches!(document.node(ids[15]), Some(N::Tuple { children })
            if document.edge_range(children.clone()).is_some_and(|children| matches!(children, [binary, integer]
                if matches!(document.node(*binary), Some(N::Binary { bytes })
                    if document.byte_range(bytes.clone()) == Some(b"x".as_slice()))
                    && matches!(document.node(*integer), Some(N::Integer(-9)))))));
    }
}
