//! Scalar classes, normalization values and independent leaf lifetimes.

use super::{A, E, I, S, attrs, states, valid_attrs, valid_list, valid_scalar, valid_states};

#[test]
fn metadata_integer_and_float_classes_preserve_exact_boundaries() {
    let padded_min = [0, 0, 0, 0, 0, 0, 0, 128, 0, 0];
    let zero = [0; 12];
    let mut huge = [0; 26];
    huge[25] = 1;
    let big: &[(bool, &[u8], Option<i64>)] = &[
        (true, &padded_min, Some(i64::MIN)),
        (false, &padded_min, None),
        (true, &zero, Some(0)),
        (false, &huge, None),
    ];
    let integers = [
        (I::Integer(i128::from(i64::MIN)), Some(i64::MIN)),
        (I::Integer(i128::from(i64::MAX)), Some(i64::MAX)),
        (I::Integer(i128::from(i64::MIN) - 1), None),
        (I::Integer(i128::from(i64::MAX) + 1), None),
    ]
    .into_iter()
    .chain(big.iter().map(|&(negative, little_endian, expected)| {
        (
            I::BigInteger {
                negative,
                little_endian,
            },
            expected,
        )
    }));
    for (input, expected) in integers {
        assert_eq!(valid_scalar(input), expected.is_some());
        let result = attrs(I::Map(&[(I::Binary(b"k"), input)]));
        match expected {
            Some(expected) => assert!(matches!(
                result, Ok(out) if out.entries.len() == 1
                    && matches!(out.entries[0].1, A::Scalar(S::Integer(n)) if n == expected)
            )),
            None => assert!(matches!(
                result,
                Err(E::SourceReturned(
                    "ERR flow attribute integer must fit in signed 64 bits"
                ))
            )),
        }
    }
    for (bits, finite) in [
        (0_u64, true),
        (0x8000_0000_0000_0000, true),
        (1, true),
        (0x7fef_ffff_ffff_ffff, true),
        (0x7ff0_0000_0000_0000, false),
        (0xfff0_0000_0000_0000, false),
        (0x7ff8_0000_0000_0001, false),
    ] {
        let input = I::Float(f64::from_bits(bits));
        assert_eq!(valid_scalar(input), finite);
        let result = attrs(I::Map(&[(I::Binary(b"k"), input)]));
        assert!(match result {
            Ok(out) =>
                finite
                    && out.entries.len() == 1
                    && matches!(out.entries[0].1, A::Scalar(S::Float(n)) if n.to_bits() == bits),
            Err(E::SourceReturned("ERR flow attribute float must be finite")) => !finite,
            _ => false,
        });
    }
}

#[test]
fn attribute_normalization_preserves_atoms_opaque_binaries_and_last_list_occurrences() {
    empty_metadata_classes();
    scalar_atom_aliases();
    list_atom_aliases_and_opaque_values();
}

fn empty_metadata_classes() {
    for input in [I::Nil, I::Atom("nil"), I::Map(&[])] {
        assert_eq!(valid_attrs(input), matches!(input, I::Map(_)));
        assert_eq!(valid_states(input), matches!(input, I::Map(_)));
        assert!(matches!(attrs(input), Ok(out) if out.entries.is_empty()));
        assert!(matches!(states(input), Ok(out) if out.states.is_empty()));
        assert!(
            matches!(states(I::Map(&[(I::Binary(b"s"), input)])), Ok(out)
            if out.states.len() == 1 && out.states[0].1.is_empty())
        );
    }
}

fn scalar_atom_aliases() {
    for (input, tag) in [
        (I::Nil, 0),
        (I::Atom("nil"), 0),
        (I::Boolean(true), 1),
        (I::Atom("true"), 1),
        (I::Boolean(false), 2),
        (I::Atom("false"), 2),
    ] {
        assert!(valid_scalar(input));
        assert!(matches!(attrs(I::Map(&[(I::Binary(b"k"), input)])), Ok(out)
        if out.entries.len() == 1 && match out.entries[0].1 {
            A::Scalar(S::Nil) => tag == 0,
            A::Scalar(S::Boolean(value)) => tag == if value { 1 } else { 2 },
            _ => false,
        }));
    }
}

fn list_atom_aliases_and_opaque_values() {
    let aliases = [
        I::Atom("a"),
        I::Binary(b"b"),
        I::Binary(b"a"),
        I::Atom("c"),
        I::Atom("b"),
    ];
    assert!(
        matches!(attrs(I::Map(&[(I::Binary(b"k"), I::List(&aliases))])), Ok(out)
        if out.entries.len() == 1 && matches!(&out.entries[0].1, A::List(values)
            if values.as_slice() == [b"a".as_slice(), b"c".as_slice(), b"b".as_slice()]))
    );
    let atoms = [
        I::Nil,
        I::Atom("nil"),
        I::Boolean(true),
        I::Atom("true"),
        I::Boolean(false),
        I::Atom("false"),
    ];
    assert!(!valid_list(I::List(&atoms)));
    assert!(
        matches!(attrs(I::Map(&[(I::Binary(b"k"), I::List(&atoms))])), Ok(out)
        if out.entries.len() == 1 && matches!(&out.entries[0].1, A::List(values)
            if values.as_slice() == [b"nil".as_slice(), b"true".as_slice(), b"false".as_slice()]))
    );
    let opaque = [I::Binary(b"\xff"), I::Binary(b"")];
    let rows = [
        (I::Binary(b"l"), I::List(&opaque)),
        (I::Binary(b"s"), I::Binary(b"\xff")),
    ];
    assert!(
        matches!(attrs(I::Map(&rows)), Ok(out) if out.entries.len() == 2
        && matches!(&out.entries[0].1, A::List(v) if v.as_slice() == [b"\xff".as_slice(), b"".as_slice()])
        && matches!(out.entries[1].1, A::Scalar(S::Binary(b"\xff"))))
    );
    assert!(
        matches!(attrs(I::Map(&[(I::Atom("k"), I::Atom("worker"))])), Ok(out)
        if out.entries.len() == 1 && out.entries[0].0 == "k"
            && matches!(out.entries[0].1, A::Scalar(S::Binary(b"worker"))))
    );
    assert!(!valid_scalar(I::Atom("worker")));
}

#[test]
fn metadata_outputs_borrow_leaves_and_keep_input_graphs_unchanged() {
    let attribute_key = b" k ".to_vec();
    let state_name = b" queued ".to_vec();
    let state_key = b" worker ".to_vec();
    let binary = vec![255, 0, 7];
    let member = vec![254, 0, 8];
    let (attributes, state_meta) = {
        let members = [I::Binary(&member)];
        let rows = [
            (I::Binary(&attribute_key), I::List(&members)),
            (I::Binary(b"v"), I::Binary(&binary)),
        ];
        let inner = [(I::Binary(&state_key), I::Binary(&binary))];
        let outer = [(I::Binary(&state_name), I::Map(&inner))];
        (attrs(I::Map(&rows)), states(I::Map(&outer)))
    };
    assert!(attributes.is_ok());
    assert!(state_meta.is_ok());
    if let (Ok(attributes), Ok(state_meta)) = (attributes, state_meta) {
        assert_borrowed_metadata(
            &attributes,
            &state_meta,
            &attribute_key,
            &state_name,
            &state_key,
            &binary,
            &member,
        );
    }
    assert_eq!(attribute_key, b" k ");
    assert_eq!(state_name, b" queued ");
    assert_eq!(state_key, b" worker ");
    assert_eq!(binary, [255, 0, 7]);
    assert_eq!(member, [254, 0, 8]);
}

fn assert_borrowed_metadata(
    attributes: &wf_domain::metadata::OriginalAttributes<'_>,
    state_meta: &wf_domain::metadata::OriginalStateMeta<'_>,
    attribute_key: &[u8],
    state_name: &[u8],
    state_key: &[u8],
    binary: &[u8],
    member: &[u8],
) {
    assert_eq!(attributes.entries.len(), 2);
    assert_eq!(state_meta.states.len(), 1);
    assert_eq!(state_meta.states[0].1.len(), 1);
    assert_eq!(attributes.entries[0].0, "k");
    assert_eq!(
        attributes.entries[0].0.as_ptr(),
        attribute_key[1..2].as_ptr()
    );
    assert!(matches!(&attributes.entries[0].1, A::List(values) if values.len() == 1));
    if let A::List(values) = &attributes.entries[0].1 {
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], member);
        assert_eq!(values[0].as_ptr(), member.as_ptr());
    }
    assert!(matches!(attributes.entries[1].1, A::Scalar(S::Binary(v))
        if v == binary && v.as_ptr() == binary.as_ptr()));
    assert_eq!(state_meta.states[0].0, "queued");
    assert_eq!(state_meta.states[0].0.as_ptr(), state_name[1..7].as_ptr());
    assert_eq!(state_meta.states[0].1[0].0, "worker");
    assert_eq!(
        state_meta.states[0].1[0].0.as_ptr(),
        state_key[1..7].as_ptr()
    );
    assert!(matches!(state_meta.states[0].1[0].1, S::Binary(v)
        if v == binary && v.as_ptr() == binary.as_ptr()));
}
