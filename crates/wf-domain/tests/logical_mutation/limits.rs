use wf_domain::value::OriginalValueEncodeError as VE;

use super::{C, E, I, Input, L, S, TestResult, V, fixtures as f, prepare};

#[test]
fn logical_caller_enforces_independent_aggregate_and_value_limits() -> TestResult {
    let nodes = f::order_nodes();
    let value = V::Value {
        input: I {
            bytes: f::ORDER_BYTES,
            nodes: &nodes,
            edges: f::ORDER_EDGES,
            root: 7,
        },
        init_pid: None,
    };
    let fields = [("type", V::Scalar(S::Bytes(b"type"))), ("payload", value)];
    let healthy = f::healthy("create", &fields)?;
    let input = Input {
        operation: "create",
        id: b"id",
        fields: &fields,
    };
    let limits = f::limits();
    for cut in [
        L {
            max_work_units: 1,
            ..limits
        },
        L {
            max_temporary_bytes: 1,
            ..limits
        },
    ] {
        assert!(matches!(
            prepare(input, &[], None, f::stamp()?, cut),
            Err(E::Limit)
        ));
    }
    assert!(matches!(
        prepare(
            input,
            &[],
            None,
            f::stamp()?,
            L {
                max_raw_bytes: 1,
                ..limits
            }
        ),
        Err(E::Caller(C::RawBytes))
    ));
    for invalid in [
        L {
            max_raw_bytes: 0,
            ..limits
        },
        L {
            max_work_units: 0,
            ..limits
        },
        L {
            max_temporary_bytes: 0,
            ..limits
        },
    ] {
        assert!(matches!(
            prepare(input, &[], None, f::stamp()?, invalid),
            Err(E::InvalidLimits)
        ));
    }
    value_limits(input, limits)?;
    let exact = L {
        max_work_units: healthy.work_units(),
        max_temporary_bytes: healthy.temporary_bytes(),
        ..limits
    };
    assert!(prepare(input, &[], None, f::stamp()?, exact).is_ok());
    two_values()?;
    Ok(())
}

fn value_limits(input: Input<'_>, limits: L) -> TestResult {
    for axis in 0..7 {
        let mut cut = limits;
        match axis {
            0 => cut.values.max_input_bytes = 1,
            1 => cut.values.max_nodes = 1,
            2 => cut.values.max_edges = 1,
            3 => cut.values.max_depth = 1,
            4 => cut.values.max_encoded_bytes = 1,
            5 => cut.values.max_work_units = 1,
            _ => cut.values.max_temporary_bytes = 1,
        }
        assert!(matches!(
            prepare(input, &[], None, f::stamp()?, cut),
            Err(E::Value {
                field: "payload",
                source: VE::Limit
            })
        ));
        let mut invalid = cut;
        match axis {
            0 => invalid.values.max_input_bytes = 0,
            1 => invalid.values.max_nodes = 0,
            2 => invalid.values.max_edges = 0,
            3 => invalid.values.max_depth = 0,
            4 => invalid.values.max_encoded_bytes = 0,
            5 => invalid.values.max_work_units = 0,
            _ => invalid.values.max_temporary_bytes = 0,
        }
        assert!(matches!(
            prepare(input, &[], None, f::stamp()?, invalid),
            Err(E::InvalidLimits)
        ));
    }
    Ok(())
}

fn two_values() -> TestResult {
    let fields = [
        ("lease_token", V::Scalar(S::Bytes(b"lease"))),
        ("fencing_token", V::Scalar(S::Integer(7))),
        ("payload", V::Scalar(S::Bytes(b"payload"))),
        ("result", V::Scalar(S::Bytes(b"result"))),
    ];
    let complete = f::healthy("complete", &fields)?;
    let input = Input {
        operation: "complete",
        id: b"id",
        fields: &fields,
    };
    let exact = L {
        max_work_units: complete.work_units(),
        max_temporary_bytes: complete.temporary_bytes(),
        ..f::limits()
    };
    assert!(prepare(input, &[], None, f::stamp()?, exact).is_ok());
    for short in [
        L {
            max_work_units: exact.max_work_units - 1,
            ..exact
        },
        L {
            max_temporary_bytes: exact.max_temporary_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            prepare(input, &[], None, f::stamp()?, short),
            Err(E::Limit
                | E::Value {
                    source: VE::Limit,
                    ..
                })
        ));
    }
    Ok(())
}
