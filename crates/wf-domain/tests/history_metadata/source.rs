use std::borrow::Cow;

use super::{I, N, TestResult, fixtures as f};

#[test]
fn source_history_metadata_ignores_unsupported_compound_values() -> TestResult {
    f::ready()?;
    // Exact FlowCodecTest441: float1.5, nested map/list/tuple, one equality.
    let nodes = [
        N::Binary { bytes: 0..5 },
        N::FloatBits(1.5_f64.to_bits()),
        N::Binary { bytes: 5..8 },
        N::Atom {
            bytes: 17..23,
            utf8: true,
        },
        N::Atom {
            bytes: 23..27,
            utf8: true,
        },
        N::Map { entries: 0..2 },
        N::Binary { bytes: 8..12 },
        N::Binary { bytes: 17..23 },
        N::EmptyList,
        N::List {
            heads: 2..3,
            tail: 8,
        },
        N::Binary { bytes: 12..17 },
        N::Tuple { children: 3..5 },
        N::Map { entries: 5..13 },
    ];
    let result = f::healthy(
        I {
            bytes: b"floatmaplisttuplenestedtrue",
            nodes: &nodes,
            edges: &[3, 4, 7, 3, 4, 0, 1, 6, 9, 2, 5, 10, 11],
            root: 12,
        },
        f::limits(),
    )?;
    assert_eq!(
        result
            .fields()
            .iter()
            .map(|field| (field.key.as_ref(), field.value.as_ref()))
            .collect::<Vec<_>>(),
        vec![(b"float".as_slice(), b"1.5".as_slice())]
    );
    Ok(())
}

#[test]
fn history_metadata_preserves_order_nil_and_borrowed_bytes() -> TestResult {
    f::ready()?;
    let bytes = b"xniltrue\0\xff";
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::Binary { bytes: 0..1 },
        N::Atom {
            bytes: 1..4,
            utf8: true,
        },
        N::Atom {
            bytes: 4..8,
            utf8: true,
        },
        N::Binary { bytes: 8..10 },
        N::EmptyList,
        N::Map { entries: 0..12 },
    ];
    let value = f::healthy(
        I {
            bytes,
            nodes: &nodes,
            edges: &[0, 4, 2, 2, 3, 3, 5, 4, 1, 2, 4, 5],
            root: 6,
        },
        f::limits(),
    )?;
    let fields = value.fields();
    assert_eq!(
        fields
            .iter()
            .map(|field| (field.key.as_ref(), field.value.as_ref()))
            .collect::<Vec<_>>(),
        vec![
            (b"x".as_slice(), b"\0\xff".as_slice()),
            (b"nil", b""),
            (b"true", b"true"),
            (b"x", b""),
        ]
    );
    assert!(
        fields
            .iter()
            .all(|field| matches!(field.key, Cow::Borrowed(_))
                && matches!(field.value, Cow::Borrowed(_)))
    );
    assert!(std::ptr::eq(fields[0].key.as_ref(), &raw const bytes[..1]));
    assert!(std::ptr::eq(fields[0].value.as_ref(), &raw const bytes[8..]));
    assert!(std::ptr::eq(fields[3].key.as_ref(), &raw const bytes[..1]));
    let pointer = fields.as_ptr();
    let moved = value.into_fields();
    assert_eq!(moved.as_ptr(), pointer);
    for node in [
        N::EmptyList,
        N::Integer(1),
        N::Tuple {
            children: usize::MAX..usize::MAX,
        },
    ] {
        let nodes = [node];
        let empty = f::healthy(
            I {
                bytes: &[],
                nodes: &nodes,
                edges: &[],
                root: 0,
            },
            f::limits(),
        )?;
        assert!(empty.fields().is_empty());
    }
    Ok(())
}
