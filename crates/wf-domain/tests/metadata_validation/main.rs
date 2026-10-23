//! Pinned `MetadataValidationTest` at 66586c596e271408d9184af32eb31cd7218c126e.
//! Source async:true, with no flow tag or local setup. The first three tests
//! retain all 28 observations in source order (19/6/3).
//! Source IDs: ca42c6b45ed50200, 6fbaf005c2b4dc11, d482759cd391b8c3.
//! Captured maps have unique exact source keys and actual source reduce order.
//! Fixture allocation is outside the production normalizers' resource claims.

#![forbid(unsafe_code)]

use wf_domain::metadata::{
    OriginalAttributeValue as A, OriginalMetadataError as E, OriginalMetadataInput as I,
    OriginalMetadataScalar as S, normalize_original_attributes as attrs,
    normalize_original_state_meta as states, valid_original_attribute_list as valid_list,
    valid_original_attribute_scalar as valid_scalar, valid_original_attributes as valid_attrs,
    valid_original_state_meta as valid_states,
};

mod bounds;
mod normalization;
mod values;

#[test]
fn attributes_identify_canonical_persisted_metadata_without_rebuilding_it() {
    let list = [I::Binary(b"one"), I::Binary(b"two")];
    let canonical = [
        (I::Binary(b"binary"), I::Binary(b"value")),
        (I::Binary(b"boolean"), I::Boolean(true)),
        (I::Binary(b"float"), I::Float(1.5)),
        (I::Binary(b"integer"), I::Integer(-1)),
        (I::Binary(b"list"), I::List(&list)),
        (I::Binary(b"null"), I::Nil),
    ];
    assert!(valid_attrs(I::Map(&canonical)));
    let x256 = vec![b'x'; 256];
    for scalar in [
        I::Nil,
        I::Boolean(false),
        I::Boolean(true),
        I::Integer(-1),
        I::Float(1.5),
        I::Binary(b""),
        I::Binary(&x256),
    ] {
        assert!(valid_scalar(scalar));
    }
    assert!(valid_list(I::List(&list)));
    assert!(!valid_scalar(I::List(&[I::Binary(b"one")])));
    assert!(!valid_scalar(I::Binary(&[b'x'; 257])));
    assert!(!valid_list(I::List(&[])));
    let duplicate = [I::Binary(b"same"), I::Binary(b"same")];
    assert!(!valid_list(I::List(&duplicate)));
    let mixed = [I::Binary(b"one"), I::Integer(2)];
    assert!(!valid_list(I::List(&mixed)));
    assert!(!valid_attrs(I::Map(&[(I::Binary(b"empty"), I::List(&[]))])));
    assert!(!valid_attrs(I::Map(&[(
        I::Binary(b"duplicate"),
        I::List(&duplicate)
    )])));
    assert!(!valid_attrs(I::Map(&[(
        I::Binary(b" padded"),
        I::Binary(b"value")
    )])));
    assert!(!valid_attrs(I::Map(&[(
        I::Binary(b"\xff"),
        I::Binary(b"value")
    )])));
    assert!(!valid_attrs(I::Map(&[(
        I::Atom("atom_key"),
        I::Binary(b"value")
    )])));
}

#[test]
fn state_metadata_identifies_canonical_scalar_only_persisted_metadata() {
    let inner = [
        (I::Binary(b"binary"), I::Binary(b"value")),
        (I::Binary(b"boolean"), I::Boolean(false)),
        (I::Binary(b"float"), I::Float(1.5)),
        (I::Binary(b"integer"), I::Integer(-1)),
        (I::Binary(b"null"), I::Nil),
    ];
    assert!(valid_states(I::Map(&[(
        I::Binary(b"queued"),
        I::Map(&inner)
    )])));
    let invalid = [I::Binary(b"invalid")];
    let list = [(I::Binary(b"list"), I::List(&invalid))];
    assert!(!valid_states(I::Map(&[(
        I::Binary(b"queued"),
        I::Map(&list)
    )])));
    let worker = [(I::Binary(b"worker"), I::Binary(b"one"))];
    assert!(!valid_states(I::Map(&[(
        I::Binary(b" queued"),
        I::Map(&worker)
    )])));
    let padded = [(I::Binary(b" worker"), I::Binary(b"one"))];
    assert!(!valid_states(I::Map(&[(
        I::Binary(b"queued"),
        I::Map(&padded)
    )])));
    assert!(!valid_states(I::Map(&[(
        I::Binary(b"\xff"),
        I::Map(&worker)
    )])));
    let opaque = [(I::Binary(b"\xff"), I::Binary(b"one"))];
    assert!(!valid_states(I::Map(&[(
        I::Binary(b"queued"),
        I::Map(&opaque)
    )])));
}

#[test]
fn normalization_rejects_names_that_cannot_be_addressed_by_fql() {
    assert!(matches!(
        attrs(I::Map(&[(I::Binary(b"\xff"), I::Binary(b"value"))])),
        Err(E::SourceReturned(_))
    ));
    let worker = [(I::Binary(b"worker"), I::Binary(b"one"))];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"\xff"), I::Map(&worker))])),
        Err(E::SourceReturned(_))
    ));
    let opaque = [(I::Binary(b"\xff"), I::Binary(b"one"))];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"queued"), I::Map(&opaque))])),
        Err(E::SourceReturned(_))
    ));
}
