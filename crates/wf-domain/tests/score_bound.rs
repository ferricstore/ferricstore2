//! Pinned `ScoreBoundTest` at 66586c596e271408d9184af32eb31cd7218c126e.
//! Source: `apps/ferricstore/test/ferricstore/flow/score_bound_test.exs`.
//! Source async:true/flow, with no local setup. The first three cases retain
//! all seven original observations in order, including bound kinds and bits.
//! Source IDs: 302351265d6e869f, ad80a44b66da3f16, 1be2c83f0aed7528.
//! Further cases cover the binary helper, not public query or I/O behavior.

#![forbid(unsafe_code)]

use wf_domain::options::{
    OriginalScoreBound as Bound, OriginalScoreBoundError as Error, parse_original_score_bound,
};

#[test]
fn parse_supports_infinities() {
    assert_eq!(parse_original_score_bound(b"-inf"), Ok(Bound::NegInf));
    assert_eq!(parse_original_score_bound(b"+inf"), Ok(Bound::PosInf));
}

#[test]
fn parse_supports_inclusive_and_exclusive_float_bounds() {
    assert!(matches!(
        parse_original_score_bound(b"42"),
        Ok(Bound::Inclusive(value)) if value.to_bits() == 0x4045_0000_0000_0000
    ));
    assert!(matches!(
        parse_original_score_bound(b"-1.5"),
        Ok(Bound::Inclusive(value)) if value.to_bits() == 0xbff8_0000_0000_0000
    ));
    assert!(matches!(
        parse_original_score_bound(b"(3.25"),
        Ok(Bound::Exclusive(value)) if value.to_bits() == 0x400a_0000_0000_0000
    ));
}

#[test]
fn parse_rejects_non_float_bounds() {
    assert_eq!(
        parse_original_score_bound(b"bad"),
        Err(Error::Returned("ERR min or max is not a float"))
    );
    assert_eq!(
        parse_original_score_bound(b"(bad"),
        Err(Error::Returned("ERR min or max is not a float"))
    );
}

#[test]
fn score_bound_preserves_prefix_grammar_and_error_origins() {
    let valid: &[(&[u8], bool, u64)] = &[
        (b"+1", false, 0x3ff0_0000_0000_0000),
        (b"001.50", false, 0x3ff8_0000_0000_0000),
        (b"1e2", false, 0x4059_0000_0000_0000),
        (b"-2E+2", false, 0xc069_0000_0000_0000),
        (b"1.25e+2", false, 0x405f_4000_0000_0000),
        (b"1E-1", false, 0x3fb9_9999_9999_999a),
        (b"(2.5e-1", true, 0x3fd0_0000_0000_0000),
        (b"(+0.5", true, 0x3fe0_0000_0000_0000),
    ];
    for &(input, exclusive, bits) in valid {
        let actual = parse_original_score_bound(input);
        assert!(
            match (actual, exclusive) {
                (Ok(Bound::Inclusive(value)), false) | (Ok(Bound::Exclusive(value)), true) =>
                    value.to_bits() == bits,
                _ => false,
            },
            "input {input:?}: {actual:?}"
        );
    }

    let invalid: &[&[u8]] = &[
        b"",
        b"(",
        b"+",
        b"-",
        b"(+",
        b"(-",
        b".5",
        b"-.5",
        b"1.",
        b"1.e2",
        b"1e",
        b"1e+",
        b"1e-",
        b"1e2e3",
        b"1.2.3",
        b"1e2.0",
        b"1x",
        b"1e2x",
        b"inf",
        b"nan",
        b"NaN",
        b"Infinity",
        b"-Inf",
        b"+Inf",
        b"-inf0",
        b"+inf0",
        b"(+inf",
        b"(-inf",
        b"((1",
        b" 1",
        b"1 ",
        b"1\n",
        b"\t1",
        b"0x1",
        b"1_0",
        b"1,5",
        b"--1",
        b"+-1",
        b"\xff1",
        b"1\xff",
        b"(1\xff",
        b"1\0",
        b"\xd9\xa1",
        b"\xef\xbc\x91",
    ];
    for &input in invalid {
        assert_eq!(
            parse_original_score_bound(input),
            Err(Error::Returned("ERR min or max is not a float")),
            "input {input:?}"
        );
    }
}

#[test]
fn score_bound_preserves_ieee_zero_boundaries_and_rounding_bits() {
    let cases: &[(&[u8], bool, u64)] = &[
        (b"0", false, 0x0000_0000_0000_0000),
        (b"+0", false, 0x0000_0000_0000_0000),
        (b"-0", false, 0x8000_0000_0000_0000),
        (b"0.0", false, 0x0000_0000_0000_0000),
        (b"-0.0", false, 0x8000_0000_0000_0000),
        (b"1e-400", false, 0x0000_0000_0000_0000),
        (b"-1e-400", false, 0x8000_0000_0000_0000),
        (b"5e-324", false, 0x0000_0000_0000_0001),
        (b"-5e-324", false, 0x8000_0000_0000_0001),
        (b"2.2250738585072014e-308", false, 0x0010_0000_0000_0000),
        (b"1.7976931348623157e308", false, 0x7fef_ffff_ffff_ffff),
        (
            b"1.00000000000000011102230246251565404236316680908203125",
            false,
            0x3ff0_0000_0000_0000,
        ),
        (
            b"1.00000000000000033306690738754696212708950042724609375",
            false,
            0x3ff0_0000_0000_0002,
        ),
        (b"(-0", true, 0x8000_0000_0000_0000),
        (b"(-1e-400", true, 0x8000_0000_0000_0000),
        (b"(-5e-324", true, 0x8000_0000_0000_0001),
    ];
    for &(input, exclusive, bits) in cases {
        let actual = parse_original_score_bound(input);
        assert!(
            match (actual, exclusive) {
                (Ok(Bound::Inclusive(value)), false) | (Ok(Bound::Exclusive(value)), true) =>
                    value.to_bits() == bits,
                _ => false,
            },
            "input {input:?}: {actual:?}"
        );
    }
}

#[test]
fn score_bound_preserves_overflow_exception_order_before_suffix_rejection() {
    // Test-owned text is a fixture, not evidence of production allocation.
    let nines = "9".repeat(400);
    let fractional = format!("{nines}.0");
    let negative = format!("-{nines}");
    let exclusive = format!("({nines}");
    let exclusive_fractional = format!("({nines}.0");
    let junk = format!("{nines}junk");
    let fractional_junk = format!("{nines}.0junk");
    let exclusive_junk = format!("({nines}junk");
    let incomplete_e = format!("{nines}e");
    let incomplete_plus = format!("{nines}e+");
    let incomplete_minus = format!("{nines}e-");
    let fractional_incomplete = format!("{nines}.0e+");
    for input in [
        nines.as_bytes(),
        fractional.as_bytes(),
        negative.as_bytes(),
        exclusive.as_bytes(),
        exclusive_fractional.as_bytes(),
        junk.as_bytes(),
        fractional_junk.as_bytes(),
        exclusive_junk.as_bytes(),
        incomplete_e.as_bytes(),
        incomplete_plus.as_bytes(),
        incomplete_minus.as_bytes(),
        fractional_incomplete.as_bytes(),
    ] {
        assert_eq!(
            parse_original_score_bound(input),
            Err(Error::RaisedArgumentError),
            "input {input:?}"
        );
    }

    let complete_e = format!("{nines}e1");
    let complete_junk = format!("{nines}e1junk");
    let fractional_complete = format!("{nines}.0e1");
    let exclusive_complete = format!("({nines}e1junk");
    for input in [
        b"1e400".as_slice(),
        b"-1e400".as_slice(),
        b"1e400junk".as_slice(),
        b"(1e400".as_slice(),
        b"(1e400junk".as_slice(),
        b"1.7976931348623159e308".as_slice(),
        complete_e.as_bytes(),
        complete_junk.as_bytes(),
        fractional_complete.as_bytes(),
        exclusive_complete.as_bytes(),
    ] {
        assert_eq!(
            parse_original_score_bound(input),
            Err(Error::Returned("ERR min or max is not a float")),
            "input {input:?}"
        );
    }
}
