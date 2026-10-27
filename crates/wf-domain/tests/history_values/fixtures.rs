use super::{
    HistoryValueEntry, HistoryValueLimits, HistoryValueOverlay, HistoryValueReference,
    OriginalValueContext, OriginalValueDocument, OriginalValueLimits, OriginalValueNode as N,
    TestResult,
};

pub(super) fn limits() -> HistoryValueLimits {
    HistoryValueLimits {
        maximum_entries: 64,
        maximum_references: 192,
        maximum_read_results: 512,
        maximum_writes: 384,
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

pub(super) fn context() -> OriginalValueContext<'static> {
    OriginalValueContext {
        safe_atoms: Some(&[
            "atom",
            "ok",
            "binary",
            "bits",
            "float",
            "improper",
            "tail",
            "integer",
            "nested",
            "true",
            "nil",
            "tuple",
            "n",
            "nonode@nohost",
            "erlang",
            "length",
        ]),
        active_exports: Some(&[("erlang", "length", 1)]),
        local_node: Some(("nonode@nohost", 0)),
        local_functions: Some(&[]),
    }
}

pub(super) fn entry<'a, T: ?Sized>(
    source: &'a T,
    references: [HistoryValueReference<'a>; 3],
) -> HistoryValueEntry<'a, T> {
    HistoryValueEntry { source, references }
}

pub(super) fn assert_fields<T: ?Sized>(
    overlay: &HistoryValueOverlay<'_, T>,
    expected: &[(usize, &str)],
) {
    assert_eq!(overlay.writes.len(), expected.len());
    for (write, &(index, key)) in overlay.writes.iter().zip(expected) {
        assert_eq!(write.entry_index, index);
        assert_eq!(write.field.key(), key);
    }
}

pub(super) fn assert_binary(document: &OriginalValueDocument, expected: &[u8]) {
    assert!(matches!(document.root(), Some(N::Binary { bytes })
        if document.byte_range(bytes.clone()) == Some(expected)));
}

pub(super) fn compound184() -> Vec<u8> {
    // Independent pinned source golden, not produced by the tested codec.
    vec![
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
    ]
}

pub(super) fn assert_compound(document: &OriginalValueDocument) -> TestResult {
    let Some(N::Map { entries }) = document.root() else {
        return Err("expected complete compound map".into());
    };
    let ids = document.edge_range(entries.clone()).ok_or("map edges")?;
    assert_eq!(ids.len(), 16);
    for (pair, key) in ids.as_chunks::<2>().0.iter().zip([
        b"atom".as_slice(),
        b"binary",
        b"bits",
        b"float",
        b"improper",
        b"integer",
        b"nested",
        b"tuple",
    ]) {
        assert!(matches!(document.node(pair[0]), Some(N::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(key)));
    }
    assert_compound_values(document, ids)
}

fn assert_compound_values(document: &OriginalValueDocument, ids: &[usize]) -> TestResult {
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
    assert_nested(document, ids[13])?;
    assert!(matches!(document.node(ids[15]), Some(N::Tuple { children })
        if document.edge_range(children.clone()).is_some_and(|children| matches!(children, [binary, integer]
            if matches!(document.node(*binary), Some(N::Binary { bytes })
                if document.byte_range(bytes.clone()) == Some(b"x".as_slice()))
                && matches!(document.node(*integer), Some(N::Integer(-9)))))));
    Ok(())
}

fn assert_nested(document: &OriginalValueDocument, index: usize) -> TestResult {
    let Some(N::Map { entries }) = document.node(index) else {
        return Err("expected nested map".into());
    };
    let pairs = document.edge_range(entries.clone()).ok_or("nested edges")?;
    assert_eq!(pairs.len(), 2);
    assert!(matches!(document.node(pairs[0]), Some(N::Binary { bytes })
        if document.byte_range(bytes.clone()) == Some(b"k".as_slice())));
    let Some(N::List { heads, tail }) = document.node(pairs[1]) else {
        return Err("expected nested proper list".into());
    };
    assert!(matches!(document.node(*tail), Some(N::EmptyList)));
    let values = document.edge_range(heads.clone()).ok_or("list edges")?;
    assert_eq!(values.len(), 3);
    assert!(matches!(document.node(values[0]), Some(N::Integer(1))));
    for (&index, expected) in values[1..].iter().zip([b"true".as_slice(), b"nil"]) {
        assert!(matches!(document.node(index), Some(N::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(expected)));
    }
    Ok(())
}
