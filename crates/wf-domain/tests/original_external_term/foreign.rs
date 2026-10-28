use super::{C, E, I, N, TestResult, fixtures as f, prepare};
use wf_domain::value::{
    OriginalValueFunctionIdentity, prepare_original_external_term_with_init_pid,
};

#[test]
fn external_term_preparation_reuses_ordering_and_foreign_context() -> TestResult {
    f::current()?;
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
        N::ForeignPid {
            node: 1,
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
        bytes: b"mn",
        nodes: &nodes,
        edges: &[4],
        root: 5,
    };
    assert!(matches!(
        prepare(input, f::limits()),
        Err(E::UnsupportedContext { node: 5 })
    ));
    let captured = prepare_original_external_term_with_init_pid(input, 3, f::limits())?;
    let functions = [OriginalValueFunctionIdentity {
        module: "m",
        index: 0,
        arity: 1,
        uniq: [0; 16],
        old_uniq: 7,
    }];
    let context = C {
        safe_atoms: Some(&["m", "n"]),
        active_exports: None,
        local_node: Some(("n", 123)),
        local_functions: Some(&functions),
    };
    let value = f::decoded(captured.into_external_term(), &context, f::decode_limits())?;
    let Some(N::ForeignLocalFun { creator, .. }) = value.root() else {
        return Err("local function".into());
    };
    assert!(matches!(
        value.node(*creator),
        Some(N::ForeignPid {
            id: 42,
            serial: 5,
            creation: 123,
            ..
        })
    ));
    let nodes = [
        N::Integer(2),
        N::Binary { bytes: 0..1 },
        N::Integer(1),
        N::Map { entries: 0..4 },
    ];
    let encoded = f::healthy(
        I {
            bytes: b"x",
            nodes: &nodes,
            edges: &[0, 1, 2, 1],
            root: 3,
        },
        f::limits(),
    )?;
    assert_eq!(
        encoded.as_external_term(),
        b"\x83t\0\0\0\x02a\x01m\0\0\0\x01xa\x02m\0\0\0\x01x"
    );
    Ok(())
}
