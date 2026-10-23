//! Pinned `OptionsTest` at 66586c596e271408d9184af32eb31cd7218c126e.
//! Source: `apps/ferricstore/test/ferricstore/flow/options_test.exs`.
//! Source async:true/flow, with no local setup; first four cases retain all
//! 29 assertions in source order. Public transport and VM shape errors remain
//! separate from these captured proper atom-key option and unique-map views.

#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::collections::TryReserveError;

use wf_domain::options::{
    maybe_put_attr, maybe_put_default_attr, maybe_put_keyword, optional_binary,
    optional_binary_or_nil, optional_boolean, optional_non_neg_integer, required_binary,
    required_non_neg_integer, validate_ref_size,
};
use wf_domain::policy::complete::RawFlowPolicyValue as Raw;

#[test]
fn binary_options_validate_required_and_optional_strings() {
    assert_eq!(
        required_binary(&[("worker", Raw::Binary(b"w"))], "worker"),
        Ok(b"w".as_slice())
    );
    assert_eq!(
        required_binary(&[], "worker")
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow worker is required".to_owned())
    );
    assert_eq!(
        required_binary(&[("worker", Raw::Binary(b""))], "worker")
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow worker must be a non-empty string".to_owned())
    );
    assert_eq!(
        optional_binary(&[], "state", Raw::Binary(b"queued")),
        Ok(b"queued".as_slice())
    );
    assert_eq!(
        optional_binary(
            &[("state", Raw::Binary(b""))],
            "state",
            Raw::Binary(b"queued")
        )
        .err()
        .map(|error| error.to_string()),
        Some("ERR flow state must be a non-empty string".to_owned())
    );
    assert_eq!(optional_binary_or_nil(&[], "parent", Raw::Nil), Ok(None));
    assert_eq!(
        optional_binary_or_nil(&[("parent", Raw::Binary(b"p"))], "parent", Raw::Nil),
        Ok(Some(b"p".as_slice()))
    );
    assert_eq!(
        optional_binary_or_nil(&[("parent", Raw::Integer(1))], "parent", Raw::Nil)
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow parent must be a string".to_owned())
    );
}

#[test]
fn integer_and_boolean_options_preserve_flow_error_messages() {
    assert_eq!(
        required_non_neg_integer(&[("now_ms", Raw::Integer(0))], "now_ms"),
        Ok(0)
    );
    assert_eq!(
        required_non_neg_integer(&[], "now_ms")
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow now_ms is required".to_owned())
    );
    assert_eq!(
        optional_non_neg_integer(&[], "run_at_ms", Raw::Nil),
        Ok(None)
    );
    assert_eq!(
        optional_non_neg_integer(&[], "run_at_ms", Raw::Integer(10)),
        Ok(Some(10))
    );
    assert_eq!(
        optional_non_neg_integer(&[("run_at_ms", Raw::Integer(-1))], "run_at_ms", Raw::Nil)
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow run_at_ms must be a non-negative integer".to_owned())
    );
    assert_eq!(
        optional_boolean(&[], "payload", Raw::Atom("false")),
        Ok(false)
    );
    assert_eq!(
        optional_boolean(
            &[("payload", Raw::Atom("true"))],
            "payload",
            Raw::Atom("false")
        ),
        Ok(true)
    );
    assert_eq!(
        optional_boolean(
            &[("payload", Raw::Binary(b"true"))],
            "payload",
            Raw::Atom("false")
        )
        .err()
        .map(|error| error.to_string()),
        Some("ERR flow payload must be a boolean".to_owned())
    );
}

#[test]
fn integer_options_reject_values_above_flows_exact_integer_ceiling() {
    let maximum = 9_007_199_254_740_991_u64;
    assert_eq!(
        required_non_neg_integer(
            &[("fencing_token", Raw::Integer(i128::from(maximum)))],
            "fencing_token"
        ),
        Ok(maximum)
    );
    assert_eq!(
        required_non_neg_integer(
            &[("fencing_token", Raw::Integer(i128::from(maximum) + 1))],
            "fencing_token"
        )
        .err()
        .map(|error| error.to_string()),
        Some("ERR flow fencing_token exceeds maximum 9007199254740991".to_owned())
    );
    assert_eq!(
        optional_non_neg_integer(
            &[("to_ms", Raw::Integer(i128::from(maximum)))],
            "to_ms",
            Raw::Nil
        ),
        Ok(Some(maximum))
    );
    assert_eq!(
        optional_non_neg_integer(
            &[("to_ms", Raw::Integer(i128::from(maximum) + 1))],
            "to_ms",
            Raw::Nil
        )
        .err()
        .map(|error| error.to_string()),
        Some("ERR flow to_ms exceeds maximum 9007199254740991".to_owned())
    );
}

#[test]
fn ref_size_and_put_helpers_preserve_existing_semantics() -> Result<(), TryReserveError> {
    assert_eq!(validate_ref_size("payload_ref", None), Ok(()));
    let boundary = vec![b'x'; 4_096];
    assert_eq!(validate_ref_size("payload_ref", Some(&boundary)), Ok(()));
    let oversized = vec![b'x'; 4_097];
    assert_eq!(
        validate_ref_size("payload_ref", Some(&oversized))
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow payload_ref too large (max 4096 bytes)".to_owned())
    );
    let rows = [("a", 1_i64)];
    assert_eq!(maybe_put_keyword(&rows, "b", None)?.as_ref(), &rows);
    assert_eq!(
        maybe_put_keyword(&rows, "b", Some(2))?.as_ref(),
        &[("b", 2), ("a", 1)]
    );
    assert_eq!(maybe_put_attr(&rows, "b", None)?.as_ref(), &rows);
    assert_eq!(
        maybe_put_attr(&rows, "b", Some(2))?.as_ref(),
        &[("a", 1), ("b", 2)]
    );
    let empty: &[(&str, i64)] = &[];
    assert_eq!(
        maybe_put_default_attr(empty, "priority", 0, 0)?.as_ref(),
        empty
    );
    assert_eq!(
        maybe_put_default_attr(empty, "priority", 1, 0)?.as_ref(),
        &[("priority", 1)]
    );
    Ok(())
}

#[test]
fn options_select_first_exact_key_and_preserve_nil_default_presence() {
    let rows = [
        ("WORKER", Raw::Other),
        ("worker", Raw::Binary(b"first")),
        ("worker", Raw::Nil),
    ];
    assert_eq!(required_binary(&rows, "worker"), Ok(b"first".as_slice()));
    assert!(
        required_binary(
            &[("worker", Raw::Nil), ("worker", Raw::Binary(b"later"))],
            "worker"
        )
        .is_err()
    );
    assert_eq!(
        optional_binary(&[("state", Raw::Binary(b"done"))], "state", Raw::Other),
        Ok(b"done".as_slice())
    );
    for nil in [Raw::Nil, Raw::Atom("nil")] {
        assert_eq!(optional_non_neg_integer(&[], "n", nil), Ok(None));
        assert_eq!(
            optional_binary_or_nil(&[("n", nil)], "n", Raw::Other),
            Ok(None)
        );
        assert_eq!(
            optional_non_neg_integer(&[("n", nil), ("n", Raw::Integer(1))], "n", Raw::Integer(2))
                .err()
                .map(|error| error.to_string()),
            Some("ERR flow n must be a non-negative integer".to_owned())
        );
    }
    assert_eq!(
        optional_binary_or_nil(&[("parent", Raw::Binary(b""))], "parent", Raw::Other),
        Ok(Some(b"".as_slice()))
    );
    assert_eq!(
        optional_boolean(&[("b", Raw::Atom("true"))], "b", Raw::Other),
        Ok(true)
    );
    assert_eq!(
        optional_non_neg_integer(&[("n", Raw::Integer(0))], "n", Raw::Integer(i128::MAX)),
        Ok(Some(0))
    );
    assert_eq!(
        optional_non_neg_integer(&[], "n", Raw::Other)
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow n must be a non-negative integer".to_owned())
    );
    assert_eq!(
        optional_boolean(&[], "b", Raw::Binary(b"false"))
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow b must be a boolean".to_owned())
    );
}

#[test]
fn binary_options_borrow_opaque_bytes_without_implicit_ref_limits() {
    let opaque = [0xff, b' ', 0];
    let result = required_binary(&[("value", Raw::Binary(&opaque))], "value");
    assert_eq!(result, Ok(opaque.as_slice()));
    assert!(result.is_ok_and(|value| value.as_ptr() == opaque.as_ptr()));
    let large = vec![0xff; 4_097];
    assert_eq!(
        optional_binary(&[], "value", Raw::Binary(&large)),
        Ok(large.as_slice())
    );
    assert_eq!(
        required_binary(&[("value", Raw::Binary(b" \t"))], "value"),
        Ok(b" \t".as_slice())
    );
    assert_eq!(validate_ref_size("ref", Some(&[])), Ok(()));
    assert_eq!(
        validate_ref_size("ref", Some(&large))
            .err()
            .map(|error| error.to_string()),
        Some("ERR flow ref too large (max 4096 bytes)".to_owned())
    );
    for invalid in [Raw::Nil, Raw::Atom("nil"), Raw::Integer(1), Raw::Other] {
        assert!(required_binary(&[("value", invalid)], "value").is_err());
        assert!(optional_binary(&[], "value", invalid).is_err());
    }
}

#[test]
fn integer_options_handle_padded_magnitudes_negative_zero_and_sign() {
    let maximum = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x1f, 0, 0];
    let zero = [0; 40];
    for value in [
        Raw::BigInteger {
            negative: false,
            little_endian: &maximum,
        },
        Raw::Integer(9_007_199_254_740_991),
    ] {
        assert_eq!(
            required_non_neg_integer(&[("n", value)], "n"),
            Ok(9_007_199_254_740_991)
        );
    }
    for negative in [false, true] {
        assert_eq!(
            required_non_neg_integer(
                &[(
                    "n",
                    Raw::BigInteger {
                        negative,
                        little_endian: &zero,
                    }
                )],
                "n"
            ),
            Ok(0)
        );
    }
    for value in [
        Raw::BigInteger {
            negative: false,
            little_endian: &[0, 0, 0, 0, 0, 0, 0x20],
        },
        Raw::Integer(i128::MAX),
    ] {
        assert_eq!(
            required_non_neg_integer(&[("n", value)], "n")
                .err()
                .map(|error| error.to_string()),
            Some("ERR flow n exceeds maximum 9007199254740991".to_owned())
        );
    }
    for value in [
        Raw::Integer(-1),
        Raw::BigInteger {
            negative: true,
            little_endian: &[1; 17],
        },
        Raw::Atom("true"),
        Raw::Binary(b"1"),
        Raw::Other,
    ] {
        assert_eq!(
            required_non_neg_integer(&[("n", value)], "n")
                .err()
                .map(|error| error.to_string()),
            Some("ERR flow n must be a non-negative integer".to_owned())
        );
    }
}

#[test]
fn put_helpers_preserve_borrowed_rows_copy_values_and_strict_default_semantics()
-> Result<(), TryReserveError> {
    let rows = [("a", 1_i64), ("b", 2), ("b", 3), ("c", 4)];
    let unchanged = maybe_put_keyword(&rows, "b", None)?;
    assert!(matches!(unchanged, Cow::Borrowed(_)));
    assert_eq!(unchanged.as_ptr(), rows.as_ptr());
    assert_eq!(
        maybe_put_keyword(&rows, "b", Some(9))?.as_ref(),
        &[("b", 9), ("a", 1), ("c", 4)]
    );
    let attrs = [("priority", 7_i64), ("b", 2)];
    let default = maybe_put_default_attr(&attrs, "priority", 0, 0)?;
    assert!(matches!(default, Cow::Borrowed(_)));
    assert_eq!(default.as_ref(), &attrs);
    assert_eq!(
        maybe_put_attr(&attrs, "priority", Some(9))?.as_ref(),
        &[("priority", 9), ("b", 2)]
    );
    let nils: &[(&str, Option<i64>)] = &[];
    assert_eq!(
        maybe_put_default_attr(nils, "value", None, Some(0))?.as_ref(),
        &[("value", None)]
    );
    let empty: &[(&str, i64)] = &[];
    assert_eq!(
        maybe_put_attr(empty, "zero", Some(0))?.as_ref(),
        &[("zero", 0)]
    );
    assert_eq!(
        maybe_put_attr(&[], "false", Some(false))?.as_ref(),
        &[("false", false)]
    );
    assert_eq!(
        maybe_put_attr(&[], "empty", Some(b"".as_slice()))?.as_ref(),
        &[("empty", b"".as_slice())]
    );
    let payload = [0xff, 0];
    let payload_rows = [("old", payload.as_slice())];
    let copied = maybe_put_attr(&payload_rows, "new", Some(payload.as_slice()))?;
    assert_eq!(
        copied.as_ref(),
        &[("old", payload.as_slice()), ("new", payload.as_slice())]
    );
    assert!(matches!(copied, Cow::Owned(_)));
    assert!(
        copied
            .iter()
            .all(|(_, value)| value.as_ptr() == payload.as_ptr())
    );
    assert_eq!(rows, [("a", 1), ("b", 2), ("b", 3), ("c", 4)]);
    Ok(())
}
