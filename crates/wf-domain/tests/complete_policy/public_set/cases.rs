//! Whole converted views, raw size gates and exact source callback reachability.

use super::{
    CompletePolicyEncodingError, OriginalPolicySetInputRejection, PolicySchemaCharge,
    PolicySchemaError, RawFlowPolicyValue, TestResult, assert_view, limits, one, prepare,
    prepare_original_policy_set_request, rejected,
};

pub(super) fn assert_atom_keys() -> TestResult {
    let first = [RawFlowPolicyValue::Nil, RawFlowPolicyValue::Integer(1)];
    let last = [
        RawFlowPolicyValue::Atom("nil"),
        RawFlowPolicyValue::Integer(2),
    ];
    let false_key = [
        RawFlowPolicyValue::Atom("false"),
        RawFlowPolicyValue::Integer(0),
    ];
    let true_key = [
        RawFlowPolicyValue::Atom("true"),
        RawFlowPolicyValue::Integer(3),
    ];
    let rows = [
        RawFlowPolicyValue::Tuple(&first),
        RawFlowPolicyValue::Tuple(&true_key),
        RawFlowPolicyValue::Tuple(&last),
        RawFlowPolicyValue::Tuple(&false_key),
    ];
    let expected = [
        (
            RawFlowPolicyValue::Atom("false"),
            RawFlowPolicyValue::Integer(0),
        ),
        (
            RawFlowPolicyValue::Atom("nil"),
            RawFlowPolicyValue::Integer(2),
        ),
        (
            RawFlowPolicyValue::Atom("true"),
            RawFlowPolicyValue::Integer(3),
        ),
    ];
    let request = prepare(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows),
    )?;
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    for invalid in [
        RawFlowPolicyValue::Atom("done"),
        RawFlowPolicyValue::List(&[]),
    ] {
        let field = [RawFlowPolicyValue::Atom("state"), invalid];
        rejected(
            &RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&field)]),
            OriginalPolicySetInputRejection::State,
        )?;
    }
    Ok(())
}

pub(super) fn assert_original_queued_patch() -> TestResult {
    let mode = [
        RawFlowPolicyValue::Atom("mode"),
        RawFlowPolicyValue::Atom("fifo"),
    ];
    let max = [
        RawFlowPolicyValue::Atom("max_retries"),
        RawFlowPolicyValue::Integer(7),
    ];
    let retry_rows = [RawFlowPolicyValue::Tuple(&max)];
    let retry = [
        RawFlowPolicyValue::Atom("retry"),
        RawFlowPolicyValue::List(&retry_rows),
    ];
    let config = [
        RawFlowPolicyValue::Tuple(&mode),
        RawFlowPolicyValue::Tuple(&retry),
    ];
    let states = [(
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::List(&config),
    )];
    let active = [
        RawFlowPolicyValue::Atom("max_active_ms"),
        RawFlowPolicyValue::Integer(1000),
    ];
    let field = [
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::Map(&states),
    ];
    let opts = [
        RawFlowPolicyValue::Tuple(&active),
        RawFlowPolicyValue::Tuple(&field),
    ];
    let expected_retry = [(
        RawFlowPolicyValue::Atom("max_retries"),
        RawFlowPolicyValue::Integer(7),
    )];
    let expected_config = [
        (
            RawFlowPolicyValue::Atom("mode"),
            RawFlowPolicyValue::Atom("fifo"),
        ),
        (
            RawFlowPolicyValue::Atom("retry"),
            RawFlowPolicyValue::Map(&expected_retry),
        ),
    ];
    let expected_states = [(
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::Map(&expected_config),
    )];
    let expected = [
        (
            RawFlowPolicyValue::Atom("max_active_ms"),
            RawFlowPolicyValue::Integer(1000),
        ),
        (
            RawFlowPolicyValue::Atom("states"),
            RawFlowPolicyValue::Map(&expected_states),
        ),
    ];
    let request = prepare(
        &RawFlowPolicyValue::Binary(b"generation-merge-1"),
        &RawFlowPolicyValue::List(&opts),
    )?;
    assert!(!request.replace());
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    assert_second_patch()?;
    Ok(())
}

fn assert_second_patch() -> TestResult {
    let target = [
        RawFlowPolicyValue::Atom("exhausted_to"),
        RawFlowPolicyValue::Binary(b"dead"),
    ];
    let retry_rows = [RawFlowPolicyValue::Tuple(&target)];
    let retry = [
        RawFlowPolicyValue::Atom("retry"),
        RawFlowPolicyValue::List(&retry_rows),
    ];
    let queued_config = [RawFlowPolicyValue::Tuple(&retry)];
    let mode = [
        RawFlowPolicyValue::Atom("mode"),
        RawFlowPolicyValue::Atom("fifo"),
    ];
    let review_config = [RawFlowPolicyValue::Tuple(&mode)];
    let pairs = [
        (
            RawFlowPolicyValue::Binary(b"queued"),
            RawFlowPolicyValue::List(&queued_config),
        ),
        (
            RawFlowPolicyValue::Binary(b"review"),
            RawFlowPolicyValue::List(&review_config),
        ),
    ];
    let expected_retry = [(
        RawFlowPolicyValue::Atom("exhausted_to"),
        RawFlowPolicyValue::Binary(b"dead"),
    )];
    let expected_queued = [(
        RawFlowPolicyValue::Atom("retry"),
        RawFlowPolicyValue::Map(&expected_retry),
    )];
    let expected_review = [(
        RawFlowPolicyValue::Atom("mode"),
        RawFlowPolicyValue::Atom("fifo"),
    )];
    let expected_states = [
        (
            RawFlowPolicyValue::Binary(b"queued"),
            RawFlowPolicyValue::Map(&expected_queued),
        ),
        (
            RawFlowPolicyValue::Binary(b"review"),
            RawFlowPolicyValue::Map(&expected_review),
        ),
    ];
    let expected = [(
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::Map(&expected_states),
    )];
    let field = [
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::Map(&pairs),
    ];
    let opts = [RawFlowPolicyValue::Tuple(&field)];
    let request = prepare(
        &RawFlowPolicyValue::Binary(b"generation-merge-1"),
        &RawFlowPolicyValue::List(&opts),
    )?;
    assert!(!request.replace());
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    Ok(())
}

#[test]
fn original_public_set_preserves_recursive_barriers_and_callback_reachability() -> TestResult {
    assert_barriers()?;
    assert_nested_states()?;
    assert_fragments()?;
    assert_callback_reachability()?;
    assert_struct_dispatch()?;
    Ok(())
}

fn assert_barriers() -> TestResult {
    let pair = [
        RawFlowPolicyValue::Atom("n"),
        RawFlowPolicyValue::Integer(1),
    ];
    let keyword = [RawFlowPolicyValue::Tuple(&pair)];
    let tuple = [RawFlowPolicyValue::List(&keyword)];
    let map_key = [(
        RawFlowPolicyValue::Atom("nested"),
        RawFlowPolicyValue::List(&keyword),
    )];
    let source = [
        (
            RawFlowPolicyValue::Atom("tuple"),
            RawFlowPolicyValue::Tuple(&tuple),
        ),
        (
            RawFlowPolicyValue::Tuple(&tuple),
            RawFlowPolicyValue::List(&keyword),
        ),
        (
            RawFlowPolicyValue::Map(&map_key),
            RawFlowPolicyValue::List(&keyword),
        ),
    ];
    let nested = [(
        RawFlowPolicyValue::Atom("n"),
        RawFlowPolicyValue::Integer(1),
    )];
    let expected_map = [
        (source[0].0, source[0].1),
        (source[1].0, RawFlowPolicyValue::Map(&nested)),
        (source[2].0, RawFlowPolicyValue::Map(&nested)),
    ];
    let expected = [(
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Map(&expected_map),
    )];
    one("x", RawFlowPolicyValue::Map(&source), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })?;
    let list = [
        RawFlowPolicyValue::Integer(1),
        RawFlowPolicyValue::List(&keyword),
    ];
    let expected_list = [
        RawFlowPolicyValue::Integer(1),
        RawFlowPolicyValue::Map(&nested),
    ];
    let expected = [(
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::List(&expected_list),
    )];
    one("x", RawFlowPolicyValue::List(&list), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })
}

fn assert_nested_states() -> TestResult {
    let pair = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::List(&[]),
    ];
    let pairs = [RawFlowPolicyValue::Tuple(&pair)];
    let nested = [(
        RawFlowPolicyValue::Binary(b"states"),
        RawFlowPolicyValue::List(&pairs),
    )];
    let expected = [(
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Map(&nested),
    )];
    one("x", RawFlowPolicyValue::Map(&nested), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })?;
    let deletion = [(
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::Nil,
    )];
    let expected = [(
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::Map(&deletion),
    )];
    one("states", RawFlowPolicyValue::Map(&deletion), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })
}

fn assert_fragments() -> TestResult {
    let first = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Integer(1),
    ];
    let last = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Integer(2),
    ];
    let other = [
        RawFlowPolicyValue::Atom("y"),
        RawFlowPolicyValue::Integer(3),
    ];
    let prefix = [RawFlowPolicyValue::Tuple(&first)];
    let suffix = [
        RawFlowPolicyValue::Tuple(&last),
        RawFlowPolicyValue::Tuple(&other),
    ];
    let tail = RawFlowPolicyValue::List(&suffix);
    let opts = RawFlowPolicyValue::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    let expected = [
        (
            RawFlowPolicyValue::Atom("x"),
            RawFlowPolicyValue::Integer(2),
        ),
        (
            RawFlowPolicyValue::Atom("y"),
            RawFlowPolicyValue::Integer(3),
        ),
    ];
    let request = prepare(&RawFlowPolicyValue::Binary(b"r"), &opts)?;
    assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
    let prefix = [
        RawFlowPolicyValue::Integer(1),
        RawFlowPolicyValue::Integer(2),
    ];
    let tail = RawFlowPolicyValue::Nil;
    let improper = RawFlowPolicyValue::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    operational(
        "x",
        improper,
        CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority),
    )?;
    let tuple = [improper];
    let expected = [(
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Tuple(&tuple),
    )];
    one("x", RawFlowPolicyValue::Tuple(&tuple), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })
}

fn assert_callback_reachability() -> TestResult {
    let tail = RawFlowPolicyValue::Nil;
    let prefix = [RawFlowPolicyValue::Integer(1)];
    let bad = RawFlowPolicyValue::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    let first = [RawFlowPolicyValue::Atom("k"), bad];
    let last = [
        RawFlowPolicyValue::Atom("k"),
        RawFlowPolicyValue::Integer(7),
    ];
    let rows = [
        RawFlowPolicyValue::Tuple(&first),
        RawFlowPolicyValue::Tuple(&last),
    ];
    let child = [(
        RawFlowPolicyValue::Atom("k"),
        RawFlowPolicyValue::Integer(7),
    )];
    let expected = [(
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Map(&child),
    )];
    one("x", RawFlowPolicyValue::List(&rows), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })?;
    let first = [RawFlowPolicyValue::Binary(b"queued"), bad];
    let last = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::List(&[]),
    ];
    let pairs = [
        RawFlowPolicyValue::Tuple(&first),
        RawFlowPolicyValue::Tuple(&last),
    ];
    operational(
        "states",
        RawFlowPolicyValue::List(&pairs),
        CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority),
    )?;
    let last = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::Nil,
    ];
    let fallback = [pairs[0], RawFlowPolicyValue::Tuple(&last)];
    let expected_fallback = [(
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::List(&fallback),
    )];
    one("states", RawFlowPolicyValue::List(&fallback), |request| {
        assert_view(
            request.patch(),
            &RawFlowPolicyValue::Map(&expected_fallback),
        )?;
        Ok(())
    })?;
    let good = [
        RawFlowPolicyValue::Atom("mode"),
        RawFlowPolicyValue::Atom("fifo"),
    ];
    let config = [RawFlowPolicyValue::Tuple(&good)];
    let first = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::List(&[]),
    ];
    let last = [
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::List(&config),
    ];
    let pairs = [
        RawFlowPolicyValue::Tuple(&first),
        RawFlowPolicyValue::Tuple(&last),
    ];
    let cfg = [(
        RawFlowPolicyValue::Atom("mode"),
        RawFlowPolicyValue::Atom("fifo"),
    )];
    let states = [(
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::Map(&cfg),
    )];
    let expected = [(
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::Map(&states),
    )];
    one("states", RawFlowPolicyValue::List(&pairs), |request| {
        assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
        Ok(())
    })
}

fn assert_struct_dispatch() -> TestResult {
    for module in [
        RawFlowPolicyValue::Atom("Example"),
        RawFlowPolicyValue::Nil,
        RawFlowPolicyValue::Atom("nil"),
    ] {
        let map = [(RawFlowPolicyValue::Atom("__struct__"), module)];
        operational(
            "x",
            RawFlowPolicyValue::Map(&map),
            CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority),
        )?;
        let pair = [RawFlowPolicyValue::Atom("__struct__"), module];
        let keyword = [RawFlowPolicyValue::Tuple(&pair)];
        operational(
            "x",
            RawFlowPolicyValue::List(&keyword),
            CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority),
        )?;
        let tuple = [RawFlowPolicyValue::Map(&map)];
        let expected = [(
            RawFlowPolicyValue::Atom("x"),
            RawFlowPolicyValue::Tuple(&tuple),
        )];
        one("x", RawFlowPolicyValue::Tuple(&tuple), |request| {
            assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
            Ok(())
        })?;
        let key = [(
            RawFlowPolicyValue::Map(&map),
            RawFlowPolicyValue::Integer(1),
        )];
        let expected_key = [(RawFlowPolicyValue::Atom("x"), RawFlowPolicyValue::Map(&key))];
        one("x", RawFlowPolicyValue::Map(&key), |request| {
            assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected_key))?;
            Ok(())
        })?;
    }
    for (key, value) in [
        (
            RawFlowPolicyValue::Atom("__struct__"),
            RawFlowPolicyValue::Integer(1),
        ),
        (
            RawFlowPolicyValue::Atom("__struct__"),
            RawFlowPolicyValue::Binary(b"Example"),
        ),
        (
            RawFlowPolicyValue::Binary(b"__struct__"),
            RawFlowPolicyValue::Atom("Example"),
        ),
    ] {
        let map = [(key, value)];
        let expected = [(RawFlowPolicyValue::Atom("x"), RawFlowPolicyValue::Map(&map))];
        one("x", RawFlowPolicyValue::Map(&map), |request| {
            assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
            Ok(())
        })?;
    }
    Ok(())
}

fn operational(
    name: &str,
    value: RawFlowPolicyValue<'_>,
    expected: CompletePolicyEncodingError,
) -> TestResult {
    let pair = [RawFlowPolicyValue::Atom(name), value];
    let rows = [RawFlowPolicyValue::Tuple(&pair)];
    let profile = limits(1_073_741_824)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(
        matches!(prepare_original_policy_set_request(&RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows), profile, &mut charge), Err(error) if error == expected)
    );
    Ok(())
}

pub(super) fn assert_callback_order() -> TestResult {
    let reduce33 = [
        1, 3, 22, 19, 6, 23, 16, 27, 5, 21, 7, 33, 32, 28, 24, 11, 10, 20, 31, 15, 18, 2, 14, 30,
        9, 25, 4, 26, 12, 8, 17, 13, 29,
    ];
    let flat32: Vec<_> = (1..=32).collect();
    for (order, paid, peak) in [
        (flat32.as_slice(), 24886, 8152),
        (reduce33.as_slice(), 25481, 8360),
    ] {
        let names: Vec<_> = order
            .iter()
            .map(|id| format!("s{id:02}").into_bytes())
            .collect();
        let structure = [(
            RawFlowPolicyValue::Atom("__struct__"),
            RawFlowPolicyValue::Atom("Example"),
        )];
        let first = if order.len() == 32 {
            0
        } else {
            order.len() - 1
        };
        let map: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    RawFlowPolicyValue::Binary(name),
                    if index == first {
                        RawFlowPolicyValue::Map(&structure)
                    } else {
                        RawFlowPolicyValue::Nil
                    },
                )
            })
            .collect();
        let field = [RawFlowPolicyValue::Atom("x"), RawFlowPolicyValue::Map(&map)];
        let rows = [RawFlowPolicyValue::Tuple(&field)];
        let profile = limits(paid + peak)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            prepare_original_policy_set_request(
                &RawFlowPolicyValue::Binary(b"r"),
                &RawFlowPolicyValue::List(&rows),
                profile,
                &mut charge
            ),
            Err(CompletePolicyEncodingError::Schema(
                PolicySchemaError::InvalidAuthority
            ))
        ));
        assert_eq!((charge.g(), charge.p()), (paid, peak));
        let profile = limits(paid + peak - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(
            matches!(prepare_original_policy_set_request(&RawFlowPolicyValue::Binary(b"r"),
            &RawFlowPolicyValue::List(&rows),profile,&mut charge),
            Err(CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes { bytes, .. })) if bytes == paid+peak)
        );
        assert_eq!((charge.g(), charge.p()), (paid - 148, peak));
    }
    Ok(())
}
