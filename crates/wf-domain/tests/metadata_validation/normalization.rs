//! Complete name, collision and proper/improper normalization controls.

use super::{A, E, I, S, attrs, states, valid_attrs, valid_list, valid_scalar};

#[test]
fn metadata_names_preserve_unicode_trim_errors_and_reserved_rules() {
    for space in [
        '\u{9}', '\u{a}', '\u{b}', '\u{c}', '\u{d}', ' ', '\u{85}', '\u{a0}', '\u{1680}',
        '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}',
        '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}',
        '\u{205f}', '\u{3000}',
    ] {
        let name = format!("{space}k{space}");
        let row = [(I::Binary(name.as_bytes()), I::Binary(b"v"))];
        assert!(!valid_attrs(I::Map(&row)));
        assert!(
            matches!(attrs(I::Map(&row)), Ok(out) if out.entries.len() == 1 && out.entries[0].0 == "k")
        );
        let inner = [(I::Binary(b"k"), I::Binary(b"v"))];
        assert!(
            matches!(states(I::Map(&[(I::Binary(name.as_bytes()), I::Map(&inner))])), Ok(out)
            if out.states.len() == 1 && out.states[0].0 == "k")
        );
        let key = [(I::Binary(name.as_bytes()), I::Nil)];
        assert!(
            matches!(states(I::Map(&[(I::Binary(b"s"), I::Map(&key))])), Ok(out)
            if out.states.len() == 1 && out.states[0].1.len() == 1 && out.states[0].1[0].0 == "k")
        );
    }
    for mark in ['\u{feff}', '\u{200b}', '\u{180e}', '\u{2060}'] {
        let name = format!("{mark}k{mark}");
        let rows = [(I::Binary(name.as_bytes()), I::Binary(b"v"))];
        assert!(valid_attrs(I::Map(&rows)));
        assert!(
            matches!(attrs(I::Map(&rows)), Ok(out) if out.entries.len() == 1 && out.entries[0].0 == name)
        );
    }
    for (name, error) in [
        (b" ".as_slice(), "ERR flow attribute key must not be empty"),
        (b"__x".as_slice(), "ERR flow attribute key is reserved"),
        (
            b"\xff".as_slice(),
            "ERR flow attribute key must be valid UTF-8",
        ),
    ] {
        assert!(
            matches!(attrs(I::Map(&[(I::Binary(name), I::Binary(b"v"))])),
            Err(E::SourceReturned(actual)) if actual == error)
        );
    }
    let padded64 = format!("\u{a0}{}\u{a0}", "k".repeat(64));
    assert!(
        matches!(attrs(I::Map(&[(I::Binary(padded64.as_bytes()), I::Binary(b"v"))])),
        Ok(out) if out.entries.len() == 1 && out.entries[0].0.len() == 64)
    );
    let k65 = [b'k'; 65];
    assert!(matches!(
        attrs(I::Map(&[(I::Binary(&k65), I::Binary(b"v"))])),
        Err(E::SourceReturned("ERR flow attribute key too large"))
    ));
    let worker = [(I::Binary(b"k"), I::Binary(b"v"))];
    assert!(
        matches!(states(I::Map(&[(I::Binary(b"__s"), I::Map(&worker))])),
        Ok(out) if out.states.len() == 1)
    );
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"\xff"), I::Map(&worker))])),
        Err(E::SourceReturned(
            "ERR flow state_meta state must be valid UTF-8"
        ))
    ));
    let reserved = [(I::Binary(b"__k"), I::Binary(b"v"))];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"s"), I::Map(&reserved))])),
        Err(E::SourceReturned("ERR flow state_meta key is reserved"))
    ));
    let invalid = [(I::Binary(b"\xff"), I::Binary(b"v"))];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"s"), I::Map(&invalid))])),
        Err(E::SourceReturned(
            "ERR flow state_meta key must be valid UTF-8"
        ))
    ));
}

#[test]
fn metadata_validates_overwritten_rows_and_uses_last_normalized_key() {
    let invalid = [
        (I::Binary(b" k"), I::Other),
        (I::Binary(b"k"), I::Binary(b"late")),
    ];
    assert!(matches!(
        attrs(I::Map(&invalid)),
        Err(E::SourceReturned(
            "ERR flow attribute value must be scalar or string list"
        ))
    ));
    let valid = [
        (I::Binary(b" k"), I::Binary(b"early")),
        (I::Binary(b"k"), I::Binary(b"late")),
    ];
    assert!(
        matches!(attrs(I::Map(&valid)), Ok(out) if out.entries.len() == 1
        && out.entries[0].0 == "k" && matches!(out.entries[0].1, A::Scalar(S::Binary(b"late"))))
    );
    let order = [
        (I::Atom("z"), I::Binary(b"first")),
        (I::Binary(b"a"), I::Binary(b"middle")),
        (I::Binary(b"z"), I::Binary(b"last")),
    ];
    assert!(
        matches!(attrs(I::Map(&order)), Ok(out) if out.entries.len() == 2
        && out.entries[0].0 == "a" && out.entries[1].0 == "z"
        && matches!(out.entries[1].1, A::Scalar(S::Binary(b"last"))))
    );
    let first_bad_value = [(I::Binary(b" a"), I::Other), (I::Binary(b"__z"), I::Nil)];
    assert!(matches!(
        attrs(I::Map(&first_bad_value)),
        Err(E::SourceReturned(
            "ERR flow attribute value must be scalar or string list"
        ))
    ));
    let bad_first_key = [(I::Binary(b"__a"), I::Other)];
    assert!(matches!(
        attrs(I::Map(&bad_first_key)),
        Err(E::SourceReturned("ERR flow attribute key is reserved"))
    ));
    for (input, expected) in [
        (I::Nil, "nil"),
        (I::Boolean(true), "true"),
        (I::Boolean(false), "false"),
    ] {
        assert!(matches!(attrs(I::Map(&[(input, I::Nil)])), Ok(out)
            if out.entries.len() == 1 && out.entries[0].0 == expected));
    }
    let names: Vec<_> = (0..8).map(|n| n.to_string()).collect();
    let payload = [b'x'; 255];
    let rows: Vec<_> = names
        .iter()
        .map(|k| (I::Binary(k.as_bytes()), I::Binary(&payload)))
        .collect();
    let mut collision = rows.clone();
    collision.insert(0, (I::Binary(b" 0"), I::Binary(&payload)));
    assert!(matches!(attrs(I::Map(&collision)), Ok(out) if out.entries.len() == 8));
    let duplicates = [I::Binary(&payload), I::Binary(&payload)];
    let lists: Vec<_> = names
        .iter()
        .map(|k| (I::Binary(k.as_bytes()), I::List(&duplicates)))
        .collect();
    assert!(
        matches!(attrs(I::Map(&lists)), Ok(out) if out.entries.len() == 8
        && out.entries.iter().all(|(_, value)| matches!(value, A::List(values) if values.len() == 1)))
    );
}

#[test]
fn state_metadata_collisions_replace_whole_inner_maps() {
    let first = [(I::Binary(b"first"), I::Binary(b"x"))];
    let last = [(I::Binary(b"last"), I::Binary(b"y"))];
    let rows = [
        (I::Binary(b" queued"), I::Map(&first)),
        (I::Binary(b"queued"), I::Map(&last)),
    ];
    assert!(
        matches!(states(I::Map(&rows)), Ok(out) if out.states.len() == 1
        && out.states[0].0 == "queued" && out.states[0].1.len() == 1
        && out.states[0].1[0].0 == "last" && matches!(out.states[0].1[0].1, S::Binary(b"y")))
    );
    let invalid = [(I::Binary(b"bad"), I::Other)];
    let rows = [
        (I::Binary(b" queued"), I::Map(&invalid)),
        (I::Binary(b"queued"), I::Map(&last)),
    ];
    assert!(matches!(
        states(I::Map(&rows)),
        Err(E::SourceReturned(
            "ERR flow state_meta value must be scalar"
        ))
    ));
    let inner = [
        (I::Binary(b" k"), I::Binary(b"early")),
        (I::Binary(b"k"), I::Binary(b"late")),
    ];
    assert!(
        matches!(states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])), Ok(out)
        if out.states.len() == 1 && out.states[0].1.len() == 1
            && matches!(out.states[0].1[0].1, S::Binary(b"late")))
    );
    let mut huge = [0; 26];
    huge[25] = 1;
    let big = I::BigInteger {
        negative: false,
        little_endian: &huge,
    };
    for (input, kind) in [
        (I::Integer(i128::from(i64::MIN)), 0),
        (I::Float(-0.0), 1),
        (big, 2),
        (I::Float(f64::NAN), 3),
    ] {
        let inner = [(I::Binary(b"k"), input)];
        assert!(match states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])) {
            Ok(out) =>
                out.states.len() == 1
                    && out.states[0].1.len() == 1
                    && match out.states[0].1[0].1 {
                        S::Integer(value) => kind == 0 && value == i64::MIN,
                        S::Float(value) => kind == 1 && value.to_bits() == 0x8000_0000_0000_0000,
                        _ => false,
                    },
            Err(E::SourceReturned("ERR flow state_meta integer must fit in signed 64 bits")) =>
                kind == 2,
            Err(E::SourceReturned("ERR flow state_meta float must be finite")) => kind == 3,
            _ => false,
        });
    }
    let name64 = format!(" {} ", "k".repeat(64));
    let name65 = [b'k'; 65];
    let inner = [(I::Binary(name64.as_bytes()), I::Nil)];
    assert!(
        matches!(states(I::Map(&[(I::Binary(name64.as_bytes()), I::Map(&inner))])),
        Ok(out) if out.states.len() == 1 && out.states[0].0.len() == 64
            && out.states[0].1.len() == 1 && out.states[0].1[0].0.len() == 64)
    );
    assert!(matches!(
        states(I::Map(&[(I::Binary(&name65), I::Map(&inner))])),
        Err(E::SourceReturned("ERR flow state_meta state too large"))
    ));
    let inner = [(I::Binary(&name65), I::Nil)];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])),
        Err(E::SourceReturned("ERR flow state_meta key too large"))
    ));
}

#[test]
fn metadata_list_classes_preserve_improper_argument_error_and_guard_refusal() {
    let tail = I::Nil;
    for prefix in [&[I::Binary(b"one")][..], &[I::Binary(b"one"); 17][..]] {
        let improper = I::ImproperList {
            prefix,
            tail: &tail,
        };
        assert!(!valid_list(improper));
        assert!(!valid_scalar(improper));
        assert!(matches!(
            attrs(I::Map(&[(I::Binary(b"k"), improper)])),
            Err(E::RaisedArgumentError)
        ));
        let inner = [(I::Binary(b"k"), improper)];
        assert!(matches!(
            states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])),
            Err(E::SourceReturned(
                "ERR flow state_meta value must be scalar"
            ))
        ));
        let proper = [improper];
        assert!(matches!(
            attrs(I::Map(&[(I::Binary(b"k"), I::List(&proper))])),
            Err(E::SourceReturned(
                "ERR flow attribute list values must be strings"
            ))
        ));
    }
    assert!(matches!(
        attrs(I::Map(&[(I::Binary(b"k"), I::List(&[]))])),
        Err(E::SourceReturned(
            "ERR flow attribute list must not be empty"
        ))
    ));
    let proper = [I::Binary(b"one"), I::Binary(b"two")];
    assert!(
        matches!(attrs(I::Map(&[(I::Binary(b"k"), I::List(&proper))])), Ok(out)
        if out.entries.len() == 1 && matches!(&out.entries[0].1, A::List(v) if v.len() == 2))
    );
    let inner = [(I::Binary(b"k"), I::List(&proper))];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])),
        Err(E::SourceReturned(
            "ERR flow state_meta value must be scalar"
        ))
    ));
    let names: Vec<_> = (0..16).map(|n| format!("{n:02}")).collect();
    let list: Vec<_> = names
        .iter()
        .map(|name| I::Binary(name.as_bytes()))
        .collect();
    assert!(valid_list(I::List(&list)));
    assert!(
        matches!(attrs(I::Map(&[(I::Binary(b"k"), I::List(&list))])), Ok(out)
        if out.entries.len() == 1 && matches!(&out.entries[0].1, A::List(values)
            if values.len() == 16 && values[0] == b"00" && values[15] == b"15"))
    );
    let x256 = [b'x'; 256];
    assert!(
        matches!(attrs(I::Map(&[(I::Binary(b"k"), I::Binary(&x256))])), Ok(out)
        if out.entries.len() == 1 && matches!(out.entries[0].1, A::Scalar(S::Binary(v)) if v == x256.as_slice()))
    );
    let inner = [(I::Binary(b"k"), I::Binary(&x256))];
    assert!(
        matches!(states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])), Ok(out)
        if out.states.len() == 1 && out.states[0].1.len() == 1
            && matches!(out.states[0].1[0].1, S::Binary(v) if v == x256.as_slice()))
    );
    assert!(matches!(
        attrs(I::Boolean(false)),
        Err(E::SourceReturned("ERR flow attributes must be a map"))
    ));
    assert!(matches!(
        states(I::List(&[])),
        Err(E::SourceReturned("ERR flow state_meta must be a map"))
    ));
}
