use wf_domain::value::{OriginalValueEncodeError, prepare_original_flow_value_with_init_pid};

use super::{E, I, N, S, TestResult, V, fixtures as f};

#[test]
fn logical_caller_preserves_presence_primitives_and_magic_bytes() -> TestResult {
    let absent = f::healthy("create", &f::base_fields("create"))?;
    assert_eq!(f::payload(&absent.command().operation), None);
    for (value, expected) in [
        (S::Nil, b"FSV2\x02\x83w\x03nil".as_slice()),
        (S::Bool(false), b"FSV2\x02\x83w\x05false"),
        (S::Bool(true), b"FSV2\x02\x83w\x04true"),
        (S::Bytes(b""), b""),
        (S::Bytes(b"FSV1old"), b"FSV1old"),
        (S::Bytes(b"FSV2"), b"FSV2\x01FSV2"),
        (S::Bytes(b"FSV2\x01raw"), b"FSV2\x01FSV2\x01raw"),
        (S::Integer(-1), b"FSV2\x02\x83b\xff\xff\xff\xff"),
    ] {
        let fields = [
            ("type", V::Scalar(S::Bytes(b"type"))),
            ("payload", V::Scalar(value)),
        ];
        let result = f::healthy("create", &fields)?;
        assert_eq!(f::payload(&result.command().operation), Some(expected));
    }
    for value in [i128::MIN, i128::MAX] {
        let mut expected = b"FSV2\x02\x83n\x10".to_vec();
        expected.push(u8::from(value < 0));
        expected.extend_from_slice(&value.unsigned_abs().to_le_bytes());
        let fields = [
            ("type", V::Scalar(S::Bytes(b"type"))),
            ("payload", V::Scalar(S::Integer(value))),
        ];
        let result = f::healthy("create", &fields)?;
        assert_eq!(
            f::payload(&result.command().operation),
            Some(expected.as_slice())
        );
    }
    Ok(())
}

#[test]
fn logical_caller_prepares_compound_values_and_captured_functions() -> TestResult {
    let nodes = f::order_nodes();
    let input = I {
        bytes: f::ORDER_BYTES,
        nodes: &nodes,
        edges: f::ORDER_EDGES,
        root: 7,
    };
    let fields = [
        ("type", V::Scalar(S::Bytes(b"type"))),
        (
            "payload",
            V::Value {
                input,
                init_pid: None,
            },
        ),
    ];
    let result = f::healthy("create", &fields)?;
    assert_eq!(
        f::payload(&result.command().operation),
        Some(f::ORDER_STORED)
    );
    captured_function()?;
    Ok(())
}

fn captured_function() -> TestResult {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::Atom {
            bytes: 1..2,
            utf8: true,
        },
        N::ForeignPid {
            node: 1,
            id: 99,
            serial: 7,
            creation: 6,
            local_to_context: false,
        },
        N::Atom {
            bytes: 2..14,
            utf8: true,
        },
        N::ForeignPid {
            node: 3,
            id: 42,
            serial: 5,
            creation: 123,
            local_to_context: true,
        },
        N::Integer(7),
        N::ForeignLocalFun {
            module: 0,
            arity: 1,
            index: 0,
            uniq: [0; 16],
            old_index: 0,
            old_uniq: 7,
            creator: 2,
            free: 0..1,
        },
    ];
    let input = I {
        bytes: b"mnruntime@host",
        nodes: &nodes,
        edges: &[5],
        root: 6,
    };
    let capture = V::Value {
        input,
        init_pid: Some(4),
    };
    let fields = [("type", V::Scalar(S::Bytes(b"type"))), ("payload", capture)];
    let result = f::healthy("create", &fields)?;
    let expected = prepare_original_flow_value_with_init_pid(input, 4, f::limits().values)?;
    assert_eq!(
        f::payload(&result.command().operation),
        Some(expected.as_stored_bytes())
    );
    let fields = [
        ("type", V::Scalar(S::Bytes(b"type"))),
        (
            "payload",
            V::Value {
                input,
                init_pid: None,
            },
        ),
    ];
    assert!(matches!(
        super::prepare(
            super::Input {
                operation: "create",
                id: b"id",
                fields: &fields
            },
            &[],
            None,
            f::stamp()?,
            f::limits()
        ),
        Err(E::Value {
            field: "payload",
            source: OriginalValueEncodeError::UnsupportedContext { node: 6 }
        })
    ));
    Ok(())
}
