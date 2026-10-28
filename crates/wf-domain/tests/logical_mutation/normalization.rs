use super::{C, E, I, Input, N, Operation, S, TestResult, V, fixtures as f, prepare};

#[test]
fn logical_caller_prepares_all_seven_existing_mutation_kinds() -> TestResult {
    for name in [
        "create",
        "transition",
        "complete",
        "fail",
        "retry",
        "extend_lease",
        "cancel",
    ] {
        let mut fields = f::base_fields(name);
        if !matches!(name, "extend_lease" | "cancel") {
            fields.push(("payload", V::Scalar(S::Bytes(b"FSV2\x02not-a-term"))));
        }
        let secondary = match name {
            "complete" => Some("result"),
            "fail" | "retry" => Some("error"),
            "cancel" => Some("reason"),
            _ => None,
        };
        if let Some(field) = secondary {
            fields.push((field, V::Scalar(S::Integer(7))));
        }
        let result = f::healthy(name, &fields)?;
        assert_eq!(result.command().id.as_bytes(), b"id");
        assert_eq!(result.command().now.get(), 100);
        assert_eq!(result.partition(), None);
        assert!(result.evidence().auto_partition);
        assert_eq!(result.evidence().requested_now_ms, None);
        let expected = operation_matches(name, &result.command().operation);
        assert!(expected, "wrong owned operation for {name}");
        if !matches!(name, "extend_lease" | "cancel") {
            assert_eq!(
                f::payload(&result.command().operation),
                Some(b"FSV2\x01FSV2\x02not-a-term".as_slice())
            );
        }
        if secondary.is_some() {
            assert_eq!(
                f::secondary(&result.command().operation),
                Some(b"FSV2\x02\x83a\x07".as_slice())
            );
        }
    }
    Ok(())
}

fn operation_matches(name: &str, operation: &Operation) -> bool {
    match name {
        "create" => matches!(operation, Operation::Create(value)
            if value.workflow_type == b"type" && value.state == b"queued" && value.run_at.is_none()),
        "transition" => matches!(operation, Operation::Transition(value)
            if value.from == b"running" && value.to == b"waiting" && value.fence.get() == 7
            && value.lease_token.as_ref().is_some_and(|token| token.as_bytes() == b"lease")),
        _ => terminal_matches(name, operation),
    }
}

fn terminal_matches(name: &str, operation: &Operation) -> bool {
    match name {
        "complete" => {
            matches!(operation, Operation::Complete(value) if value.guard.fence.get() == 7 && value.guard.token.as_bytes() == b"lease")
        }
        "fail" => {
            matches!(operation, Operation::Fail(value) if value.guard.fence.get() == 7 && value.guard.token.as_bytes() == b"lease")
        }
        "retry" => {
            matches!(operation, Operation::RetryWithPolicy(value) if value.guard.fence.get() == 7 && value.jitter.get() == 0 && value.run_at.is_none())
        }
        "extend_lease" => {
            matches!(operation, Operation::ExtendLease(value) if value.lease_ms == 30_000 && value.guard.fence.get() == 7)
        }
        "cancel" => {
            matches!(operation, Operation::Cancel(value) if value.fence.get() == 7 && value.lease_token.is_none() && value.expected_state.is_none() && value.expected_version.is_none())
        }
        _ => false,
    }
}

#[test]
fn logical_caller_overrides_common_values_but_charges_all_captures() -> TestResult {
    let fields = [
        ("type", V::Scalar(S::Bytes(b"type"))),
        ("payload", V::Scalar(S::Bytes(b"local"))),
        ("now_ms", V::Scalar(S::Integer(77))),
        ("partition_key", V::Scalar(S::Bytes(b"p"))),
        ("priority", V::Scalar(S::Integer(2))),
    ];
    f::healthy("create", &fields)?;
    let pool = vec![b'x'; 65_536];
    let common = [
        (
            "payload",
            V::Value {
                input: I {
                    bytes: &pool,
                    nodes: &[],
                    edges: &[],
                    root: usize::MAX,
                },
                init_pid: None,
            },
        ),
        ("now_ms", V::Scalar(S::Integer(12))),
    ];
    let input = Input {
        operation: "create",
        id: b"id",
        fields: &fields,
    };
    let result = prepare(input, &common, Some(b"p"), f::stamp()?, f::limits())?;
    assert_eq!(
        f::payload(&result.command().operation),
        Some(b"local".as_slice())
    );
    assert_eq!(result.command().now.get(), 77);
    assert_eq!(result.evidence().requested_now_ms, Some(77));
    assert!(!result.evidence().auto_partition);
    assert_eq!(result.partition(), Some(b"p".as_slice()));
    assert_eq!(result.create_priority(), 2);
    let mut short = f::limits();
    short.max_raw_bytes = pool.len() - 1;
    assert!(matches!(
        prepare(input, &common, Some(b"p"), f::stamp()?, short),
        Err(E::Caller(C::RawBytes))
    ));
    assert!(matches!(
        prepare(input, &common, Some(b"other"), f::stamp()?, f::limits()),
        Err(E::Caller(C::PartitionMismatch))
    ));
    Ok(())
}

#[test]
fn logical_caller_refuses_bad_schema_before_value_preparation() -> TestResult {
    f::healthy("create", &f::base_fields("create"))?;
    let bad = V::Value {
        input: I {
            bytes: &[],
            nodes: &[],
            edges: &[],
            root: 0,
        },
        init_pid: None,
    };
    let fields = [
        ("type", V::Scalar(S::Bytes(b"type"))),
        ("payload", bad),
        ("type", V::Scalar(S::Bytes(b"again"))),
    ];
    let input = Input {
        operation: "create",
        id: b"id",
        fields: &fields,
    };
    assert!(matches!(
        prepare(input, &[], None, f::stamp()?, f::limits()),
        Err(E::Caller(C::DuplicateField))
    ));
    let fields = [("type", V::Scalar(S::Bytes(b"type"))), ("payload", bad)];
    let input = Input {
        fields: &fields,
        id: b"",
        ..input
    };
    assert!(matches!(
        prepare(input, &[], None, f::stamp()?, f::limits()),
        Err(E::Caller(C::InvalidId))
    ));
    let excessive = [("payload", bad); 9];
    let input = Input {
        id: b"id",
        fields: &excessive,
        ..input
    };
    assert!(matches!(
        prepare(input, &[], None, f::stamp()?, f::limits()),
        Err(E::Caller(C::InvalidOptions))
    ));
    let nodes = [N::Binary { bytes: 0..4 }];
    let logical = V::Value {
        input: I {
            bytes: b"type",
            nodes: &nodes,
            edges: &[],
            root: 0,
        },
        init_pid: None,
    };
    for (name, expected) in [
        ("type", C::InvalidField("type")),
        ("payload_ref", C::UnsupportedFeature("value_reference")),
        ("unknown", C::UnknownField),
    ] {
        let fields = [(name, logical)];
        let actual = prepare(
            Input {
                operation: "create",
                id: b"id",
                fields: &fields,
            },
            &[],
            None,
            f::stamp()?,
            f::limits(),
        );
        assert!(matches!(actual, Err(E::Caller(error)) if error == expected));
    }
    Ok(())
}
