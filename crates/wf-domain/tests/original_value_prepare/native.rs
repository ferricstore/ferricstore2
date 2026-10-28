//! Source-native local-fun metadata has an explicit immutable capture.

use wf_domain::value::{
    OriginalValueContext, OriginalValueFunctionIdentity, OriginalValueLimits,
    decode_original_flow_value, prepare_original_flow_value_with_init_pid as captured,
};

use super::{E, I, N, TestResult, fixtures as f, prepare};

#[test]
fn logical_value_preparation_uses_captured_initialization_pid() -> TestResult {
    let nodes = fixture_nodes(99);
    let input = I {
        bytes: b"mnruntime@host",
        nodes: &nodes,
        edges: &[5],
        root: 6,
    };
    let value = captured(input, 4, f::limits());
    assert!(
        value.is_ok(),
        "healthy captured initialization PID: {:?}",
        value.as_ref().err()
    );
    let value = value?;
    let mut body = vec![112, 0, 0, 0, 65, 1];
    body.extend_from_slice(&[0; 20]);
    body.extend_from_slice(b"\0\0\0\x01w\x01ma\0a\x07Xw\x0cruntime@host");
    for word in [42_u32, 5, 123] {
        body.extend_from_slice(&word.to_be_bytes());
    }
    body.extend_from_slice(b"a\x07");
    let expected = f::stored(&body);
    assert_eq!(value.as_stored_bytes(), expected);
    assert!(matches!(
        prepare(input, f::limits()),
        Err(E::UnsupportedContext { node: 6 })
    ));
    for bad in [3, 6, usize::MAX] {
        assert!(matches!(
            captured(input, bad, f::limits()),
            Err(E::InvalidInput)
        ));
    }
    for creator in [0, 888] {
        let changed = fixture_nodes(creator);
        let changed = I {
            nodes: &changed,
            ..input
        };
        assert_eq!(
            captured(changed, 4, f::limits())?.as_stored_bytes(),
            expected
        );
    }
    let mut short = f::limits();
    short.max_encoded_bytes = expected.len() - 1;
    assert!(matches!(captured(input, 4, short), Err(E::Limit)));
    short.max_encoded_bytes = expected.len();
    assert_eq!(captured(input, 4, short)?.as_stored_bytes(), expected);
    inconsistent_old_index();
    let plain_nodes = [N::Integer(7)];
    let plain = I {
        bytes: &[],
        nodes: &plain_nodes,
        edges: &[],
        root: 0,
    };
    assert_eq!(
        captured(plain, usize::MAX, f::limits())?.as_stored_bytes(),
        f::stored(b"a\x07")
    );
    decoded(value.into_stored_bytes())?;
    Ok(())
}

fn inconsistent_old_index() {
    let mut nodes = fixture_nodes(99);
    nodes.push(N::ForeignLocalFun {
        module: 0,
        arity: 1,
        index: 0,
        uniq: [0; 16],
        old_index: 1,
        old_uniq: 7,
        creator: 2,
        free: 0..1,
    });
    nodes.push(N::Tuple { children: 1..3 });
    let input = I {
        bytes: b"mnruntime@host",
        nodes: &nodes,
        edges: &[5, 6, 7],
        root: 8,
    };
    assert!(matches!(
        captured(input, 4, f::limits()),
        Err(E::InvalidInput)
    ));
}

fn fixture_nodes(creator: u32) -> Vec<N> {
    vec![
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
            id: creator,
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
    ]
}

fn decoded(bytes: Vec<u8>) -> TestResult {
    let functions = [OriginalValueFunctionIdentity {
        module: "m",
        index: 0,
        arity: 1,
        uniq: [0; 16],
        old_uniq: 7,
    }];
    let context = OriginalValueContext {
        safe_atoms: Some(&["m", "runtime@host", "nonode@nohost"]),
        active_exports: None,
        local_node: Some(("nonode@nohost", 0)),
        local_functions: Some(&functions),
    };
    let limits = OriginalValueLimits {
        max_encoded_bytes: 4_096,
        max_nodes: 1_024,
        max_edges: 2_048,
        max_depth: 128,
        max_work_units: 1_000_000,
    };
    let document = decode_original_flow_value(bytes, &context, limits)
        .map_err(|error| format!("native fun decode: {error:?}"))?;
    let Some(N::ForeignLocalFun { creator, .. }) = document.root() else {
        return Err("local fun root".into());
    };
    assert!(matches!(
        document.node(*creator),
        Some(N::ForeignPid {
            id: 42,
            serial: 5,
            creation: 123,
            ..
        })
    ));
    Ok(())
}
