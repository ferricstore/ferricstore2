//! Original public controls and `PolicyPatch` preparation, without dispatch.

use super::{TestResult, limits};
use metric::assert_raw;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, OriginalPolicySetInputRejection, OriginalPolicySetPatchView,
    OriginalPolicySetRequest, PolicySchemaCharge, PolicySchemaError, RawFlowPolicyValue,
    encode_original_policy_set_input_rejection_body, prepare_original_policy_set_request,
};

mod cases;
mod metric;
mod refusals;

fn prepare<'input>(
    workflow_type: &RawFlowPolicyValue<'input>,
    opts: &RawFlowPolicyValue<'input>,
) -> TestResult<OriginalPolicySetRequest<'input>> {
    let profile = limits(1_073_741_824)?;
    let mut charge = PolicySchemaCharge::new(profile);
    match prepare_original_policy_set_request(workflow_type, opts, profile, &mut charge)? {
        Ok(request) => Ok(request),
        Err(reason) => Err(format!("unexpected public rejection: {reason:?}").into()),
    }
}

fn rejected(
    workflow_type: &RawFlowPolicyValue<'_>,
    opts: &RawFlowPolicyValue<'_>,
    expected: OriginalPolicySetInputRejection,
) -> TestResult {
    let profile = limits(1_073_741_824)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let actual = prepare_original_policy_set_request(workflow_type, opts, profile, &mut charge)?;
    assert!(matches!(actual, Err(reason) if reason == expected));
    let body = encode_original_policy_set_input_rejection_body(expected, profile, &mut charge)?;
    assert_eq!(body, expected.as_bytes());
    Ok(())
}

fn assert_view(
    actual: OriginalPolicySetPatchView<'_>,
    expected: &RawFlowPolicyValue<'_>,
) -> TestResult {
    match expected {
        RawFlowPolicyValue::Map(rows) => {
            assert_eq!(actual.map_len(), Some(rows.len()));
            assert!(actual.raw().is_none());
            for (index, (key, value)) in rows.iter().enumerate() {
                let (actual_key, actual_value) = actual.map_entry(index).ok_or("map pair")?;
                assert_raw(actual_key.raw().ok_or("opaque map key")?, *key)?;
                assert_view(actual_value, value)?;
            }
            assert!(actual.map_entry(rows.len()).is_none());
        }
        RawFlowPolicyValue::List(values) => {
            assert_eq!(actual.list_len(), Some(values.len()));
            assert!(actual.raw().is_none());
            for (index, value) in values.iter().enumerate() {
                assert_view(actual.list_item(index).ok_or("list item")?, value)?;
            }
            assert!(actual.list_item(values.len()).is_none());
        }
        _ => assert_raw(actual.raw().ok_or("raw leaf")?, *expected)?,
    }
    Ok(())
}

fn one<'input>(
    name: &'input str,
    value: RawFlowPolicyValue<'input>,
    check: impl for<'view> FnOnce(OriginalPolicySetRequest<'view>) -> TestResult,
) -> TestResult {
    let pair = [RawFlowPolicyValue::Atom(name), value];
    let rows = [RawFlowPolicyValue::Tuple(&pair)];
    check(prepare(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows),
    )?)
}

#[test]
fn original_public_set_preserves_root_and_first_control_priority() -> TestResult {
    assert_roots()?;
    assert_key_bounds()?;
    assert_first_controls()?;
    assert_nil_and_false_first()?;
    cases::assert_atom_keys()?;
    let pair = [
        RawFlowPolicyValue::Atom("state"),
        RawFlowPolicyValue::Integer(1),
    ];
    let opts = [RawFlowPolicyValue::Tuple(&pair)];
    let profile = limits(1_073_741_824)?;
    let mut charge = PolicySchemaCharge::new(profile);
    // Source49bda0ec2fab73ce: flow_policy_generation_test.exs239–244/assert242.
    let result = prepare_original_policy_set_request(
        &RawFlowPolicyValue::Binary(b"generation-state-validation-1"),
        &RawFlowPolicyValue::List(&opts),
        profile,
        &mut charge,
    )?;
    assert!(matches!(
        result,
        Err(OriginalPolicySetInputRejection::State)
    ));
    let body = encode_original_policy_set_input_rejection_body(
        OriginalPolicySetInputRejection::State,
        profile,
        &mut charge,
    )?;
    assert_eq!(body, b"ERR flow state must be a string");
    Ok(())
}

fn assert_roots() -> TestResult {
    let empty = RawFlowPolicyValue::List(&[]);
    rejected(&empty, &empty, OriginalPolicySetInputRejection::Options)?;
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::Binary(b""),
        OriginalPolicySetInputRejection::Options,
    )?;
    let malformed = [RawFlowPolicyValue::Other];
    for value in [
        RawFlowPolicyValue::Nil,
        RawFlowPolicyValue::Atom("nil"),
        RawFlowPolicyValue::Integer(0),
        RawFlowPolicyValue::Map(&[]),
        RawFlowPolicyValue::Tuple(&[]),
        RawFlowPolicyValue::Other,
    ] {
        rejected(&value, &empty, OriginalPolicySetInputRejection::Options)?;
        rejected(
            &RawFlowPolicyValue::Binary(b"r"),
            &value,
            OriginalPolicySetInputRejection::Options,
        )?;
    }
    rejected(
        &RawFlowPolicyValue::Binary(b""),
        &RawFlowPolicyValue::List(&malformed),
        OriginalPolicySetInputRejection::Options,
    )?;
    rejected(
        &RawFlowPolicyValue::Binary(b""),
        &empty,
        OriginalPolicySetInputRejection::Type,
    )?;
    let binary_key = [
        RawFlowPolicyValue::Binary(b"state"),
        RawFlowPolicyValue::Nil,
    ];
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&binary_key)]),
        OriginalPolicySetInputRejection::Options,
    )?;
    let tail = RawFlowPolicyValue::Nil;
    let pair = [RawFlowPolicyValue::Atom("state"), RawFlowPolicyValue::Nil];
    let prefix = [RawFlowPolicyValue::Tuple(&pair)];
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::ImproperList {
            prefix: &prefix,
            tail: &tail,
        },
        OriginalPolicySetInputRejection::Options,
    )?;
    Ok(())
}

fn assert_nil_and_false_first() -> TestResult {
    for nil in [RawFlowPolicyValue::Nil, RawFlowPolicyValue::Atom("nil")] {
        for (name, good) in [
            ("state", RawFlowPolicyValue::Binary(b"done")),
            ("expected_generation", RawFlowPolicyValue::Integer(9)),
        ] {
            for later in [good, RawFlowPolicyValue::Integer(-1)] {
                let first = [RawFlowPolicyValue::Atom(name), nil];
                let last = [RawFlowPolicyValue::Atom(name), later];
                let rows = [
                    RawFlowPolicyValue::Tuple(&first),
                    RawFlowPolicyValue::Tuple(&last),
                ];
                let request = prepare(
                    &RawFlowPolicyValue::Binary(b"r"),
                    &RawFlowPolicyValue::List(&rows),
                )?;
                assert_eq!(request.response_state(), None);
                assert_eq!(request.expected_generation(), None);
                assert_eq!(request.patch().map_len(), Some(0));
            }
        }
    }
    for (first, last, expected) in [("false", "true", false), ("true", "false", true)] {
        let first = [
            RawFlowPolicyValue::Atom("replace"),
            RawFlowPolicyValue::Atom(first),
        ];
        let last = [
            RawFlowPolicyValue::Atom("replace"),
            RawFlowPolicyValue::Atom(last),
        ];
        let rows = [
            RawFlowPolicyValue::Tuple(&first),
            RawFlowPolicyValue::Tuple(&last),
        ];
        assert_eq!(
            prepare(
                &RawFlowPolicyValue::Binary(b"r"),
                &RawFlowPolicyValue::List(&rows)
            )?
            .replace(),
            expected
        );
    }
    Ok(())
}

fn assert_key_bounds() -> TestResult {
    let accepted = vec![b'r'; 65_522];
    let refused = vec![b'r'; 65_523];
    let request = prepare(
        &RawFlowPolicyValue::Binary(&accepted),
        &RawFlowPolicyValue::List(&[]),
    )?;
    assert_eq!(request.policy_key().len(), 65_535);
    assert_eq!(request.workflow_type(), accepted);
    assert_eq!(&request.policy_key()[..13], b"f:{f}:policy:");
    let invalid = [
        RawFlowPolicyValue::Atom("state"),
        RawFlowPolicyValue::Integer(1),
    ];
    rejected(
        &RawFlowPolicyValue::Binary(&refused),
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&invalid)]),
        OriginalPolicySetInputRejection::KeyBytes,
    )?;
    let opaque = prepare(
        &RawFlowPolicyValue::Binary(b"\xff\0"),
        &RawFlowPolicyValue::List(&[]),
    )?;
    assert_eq!(opaque.policy_key(), b"f:{f}:policy:\xff\0");
    assert_eq!(opaque.workflow_type(), b"\xff\0");
    Ok(())
}

fn assert_first_controls() -> TestResult {
    for (name, good, bad, reason) in [
        (
            "state",
            RawFlowPolicyValue::Binary(b"done"),
            RawFlowPolicyValue::Integer(1),
            OriginalPolicySetInputRejection::State,
        ),
        (
            "expected_generation",
            RawFlowPolicyValue::Integer(1),
            RawFlowPolicyValue::Integer(-1),
            OriginalPolicySetInputRejection::ExpectedGeneration,
        ),
        (
            "replace",
            RawFlowPolicyValue::Atom("true"),
            RawFlowPolicyValue::Nil,
            OriginalPolicySetInputRejection::Replace,
        ),
    ] {
        let first_good = [RawFlowPolicyValue::Atom(name), good];
        let first_bad = [RawFlowPolicyValue::Atom(name), bad];
        let rows = [
            RawFlowPolicyValue::Tuple(&first_good),
            RawFlowPolicyValue::Tuple(&first_bad),
        ];
        let request = prepare(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&rows),
        )?;
        assert_eq!(request.patch().map_len(), Some(0));
        match name {
            "state" => assert_eq!(request.response_state(), Some(b"done".as_slice())),
            "expected_generation" => assert_eq!(request.expected_generation(), Some(1)),
            _ => assert!(request.replace()),
        }
        let reversed = [rows[1], rows[0]];
        rejected(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&reversed),
            reason,
        )?;
    }
    let state = [
        RawFlowPolicyValue::Atom("state"),
        RawFlowPolicyValue::Integer(1),
    ];
    let expected = [
        RawFlowPolicyValue::Atom("expected_generation"),
        RawFlowPolicyValue::Integer(-1),
    ];
    let replace = [RawFlowPolicyValue::Atom("replace"), RawFlowPolicyValue::Nil];
    let all = [
        RawFlowPolicyValue::Tuple(&replace),
        RawFlowPolicyValue::Tuple(&expected),
        RawFlowPolicyValue::Tuple(&state),
    ];
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&all),
        OriginalPolicySetInputRejection::State,
    )?;
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&all[..2]),
        OriginalPolicySetInputRejection::ExpectedGeneration,
    )?;
    let bad_shape = [RawFlowPolicyValue::Tuple(&state), RawFlowPolicyValue::Other];
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&bad_shape),
        OriginalPolicySetInputRejection::Options,
    )?;
    Ok(())
}

#[test]
fn original_public_set_keeps_both_modes_and_unbounded_reply_state() -> TestResult {
    let empty = prepare(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&[]),
    )?;
    assert!(!empty.replace());
    assert_eq!(empty.expected_generation(), None);
    assert_eq!(empty.response_state(), None);
    for (value, mode) in [("false", false), ("true", true)] {
        one("replace", RawFlowPolicyValue::Atom(value), |request| {
            assert_eq!(request.replace(), mode);
            assert_eq!(request.patch().map_len(), Some(0));
            Ok(())
        })?;
    }
    for invalid in [
        RawFlowPolicyValue::Nil,
        RawFlowPolicyValue::Atom("nil"),
        RawFlowPolicyValue::Atom("TRUE"),
        RawFlowPolicyValue::Integer(0),
        RawFlowPolicyValue::Binary(b"false"),
        RawFlowPolicyValue::List(&[]),
    ] {
        let pair = [RawFlowPolicyValue::Atom("replace"), invalid];
        rejected(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&pair)]),
            OriginalPolicySetInputRejection::Replace,
        )?;
    }
    assert_response_states()?;
    assert_expected_generations()?;
    Ok(())
}

fn assert_response_states() -> TestResult {
    let large = vec![0xff; 262_145];
    for bytes in [b"".as_slice(), b"\xff\0".as_slice(), large.as_slice()] {
        one("state", RawFlowPolicyValue::Binary(bytes), |request| {
            let state = request.response_state().ok_or("present query")?;
            assert_eq!(state, bytes);
            assert_eq!(state.as_ptr(), bytes.as_ptr());
            assert_eq!(request.patch().map_len(), Some(0));
            Ok(())
        })?;
    }
    for nil in [RawFlowPolicyValue::Nil, RawFlowPolicyValue::Atom("nil")] {
        one("state", nil, |request| {
            assert_eq!(request.response_state(), None);
            Ok(())
        })?;
    }
    Ok(())
}

fn assert_expected_generations() -> TestResult {
    let maximum = 9_007_199_254_740_991_u64;
    let max_bytes = [255, 255, 255, 255, 255, 255, 31, 0, 0];
    for (value, expected) in [
        (RawFlowPolicyValue::Nil, None),
        (RawFlowPolicyValue::Atom("nil"), None),
        (RawFlowPolicyValue::Integer(0), Some(0)),
        (
            RawFlowPolicyValue::Integer(i128::from(maximum)),
            Some(maximum),
        ),
        (
            RawFlowPolicyValue::BigInteger {
                negative: true,
                little_endian: &[0, 0],
            },
            Some(0),
        ),
        (
            RawFlowPolicyValue::BigInteger {
                negative: false,
                little_endian: &max_bytes,
            },
            Some(maximum),
        ),
    ] {
        one("expected_generation", value, |request| {
            assert_eq!(request.expected_generation(), expected);
            Ok(())
        })?;
    }
    let huge = [1_u8; 256];
    for value in [
        RawFlowPolicyValue::Integer(-1),
        RawFlowPolicyValue::Integer(i128::from(maximum) + 1),
        RawFlowPolicyValue::Binary(b"1"),
        RawFlowPolicyValue::BigInteger {
            negative: true,
            little_endian: &[1, 0],
        },
        RawFlowPolicyValue::BigInteger {
            negative: false,
            little_endian: &huge,
        },
    ] {
        let pair = [RawFlowPolicyValue::Atom("expected_generation"), value];
        rejected(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&pair)]),
            OriginalPolicySetInputRejection::ExpectedGeneration,
        )?;
    }
    Ok(())
}

#[test]
fn original_public_set_converts_last_policy_values_and_state_pairs() -> TestResult {
    let first = [
        RawFlowPolicyValue::Atom("version"),
        RawFlowPolicyValue::Other,
    ];
    let last = [
        RawFlowPolicyValue::Atom("version"),
        RawFlowPolicyValue::Integer(2),
    ];
    let rows = [
        RawFlowPolicyValue::Tuple(&first),
        RawFlowPolicyValue::Tuple(&last),
    ];
    let expected = [(
        RawFlowPolicyValue::Atom("version"),
        RawFlowPolicyValue::Integer(2),
    )];
    let request = prepare(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows),
    )?;
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    assert_empty_states()?;
    cases::assert_original_queued_patch()?;
    Ok(())
}

fn assert_empty_states() -> TestResult {
    let states = [
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::List(&[]),
    ];
    let ordinary = [RawFlowPolicyValue::Atom("x"), RawFlowPolicyValue::List(&[])];
    let rows = [
        RawFlowPolicyValue::Tuple(&ordinary),
        RawFlowPolicyValue::Tuple(&states),
    ];
    let expected = [
        (
            RawFlowPolicyValue::Atom("states"),
            RawFlowPolicyValue::Map(&[]),
        ),
        (RawFlowPolicyValue::Atom("x"), RawFlowPolicyValue::List(&[])),
    ];
    let request = prepare(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows),
    )?;
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    let nil_pair = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::Nil,
    ];
    let state_rows = [RawFlowPolicyValue::Tuple(&nil_pair)];
    let opts = [
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::List(&state_rows),
    ];
    let expected = [(
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::List(&state_rows),
    )];
    let rows = [RawFlowPolicyValue::Tuple(&opts)];
    let request = prepare(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows),
    )?;
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    Ok(())
}

fn assert_large_governance() -> TestResult {
    // Original exhaustive governance600 UNIQUE 2000-byte prefix+i recipe.
    let names: Vec<Vec<u8>> = (1..=600)
        .map(|i| {
            let mut name = vec![b'e'; 2000];
            name.extend_from_slice(i.to_string().as_bytes());
            name
        })
        .collect();
    let values: Vec<_> = names
        .iter()
        .map(|name| RawFlowPolicyValue::Binary(name))
        .collect();
    let effects = [(
        RawFlowPolicyValue::Atom("allowed"),
        RawFlowPolicyValue::List(&values),
    )];
    let governance = [(
        RawFlowPolicyValue::Atom("effects"),
        RawFlowPolicyValue::Map(&effects),
    )];
    let gov = [
        RawFlowPolicyValue::Atom("governance"),
        RawFlowPolicyValue::Map(&governance),
    ];
    let invalid = [
        RawFlowPolicyValue::Atom("max_active_ms"),
        RawFlowPolicyValue::Integer(0),
    ];
    let opts = [
        RawFlowPolicyValue::Tuple(&invalid),
        RawFlowPolicyValue::Tuple(&gov),
    ];
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&opts),
        OriginalPolicySetInputRejection::SnapshotBytes,
    )?;
    Ok(())
}
