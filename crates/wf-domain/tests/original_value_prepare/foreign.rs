use wf_domain::value::{
    OriginalValueContext, OriginalValueFunctionIdentity, OriginalValueLimits,
    decode_original_flow_value,
};

use super::{N, TestResult, fixtures as f};

#[test]
fn logical_value_preparation_preserves_foreign_data_formats() -> TestResult {
    process_and_port()?;
    reference_and_export()?;
    local_function()?;
    Ok(())
}

fn checked(bytes: &[u8], nodes: &[N], edges: &[usize], root: usize, body: &[u8]) -> TestResult {
    let prepared = f::exact(bytes, nodes, edges, root, body)?;
    let functions = [OriginalValueFunctionIdentity {
        module: "m",
        index: 0,
        arity: 1,
        uniq: [0; 16],
        old_uniq: 7,
    }];
    let context = OriginalValueContext {
        safe_atoms: Some(&["n", "m", "erlang", "length", "nonode@nohost"]),
        active_exports: Some(&[("erlang", "length", 1)]),
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
    let decoded = decode_original_flow_value(prepared.into_stored_bytes(), &context, limits)
        .map_err(|error| format!("prepared foreign decode: {error:?}"))?;
    assert!(matches!(
        decoded.root(),
        Some(
            N::ForeignPid { .. }
                | N::ForeignPort { .. }
                | N::ForeignReference { .. }
                | N::ForeignExport { .. }
                | N::ForeignLocalFun { .. }
        )
    ));
    Ok(())
}

fn process_and_port() -> TestResult {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::ForeignPid {
            node: 0,
            id: 1,
            serial: 2,
            creation: 3,
            local_to_context: true,
        },
    ];
    checked(
        b"n",
        &nodes,
        &[],
        1,
        b"Xw\x01n\0\0\0\x01\0\0\0\x02\0\0\0\x03",
    )?;
    for id in [1_u64, u64::from(u32::MAX) + 1] {
        let nodes = [
            N::Atom {
                bytes: 0..1,
                utf8: true,
            },
            N::ForeignPort {
                node: 0,
                id,
                creation: 3,
                local_to_context: true,
            },
        ];
        // OTP28 uses V4_PORT_EXT even when the numeric ID fits u32.
        let mut body = b"xw\x01n".to_vec();
        body.extend_from_slice(&id.to_be_bytes());
        body.extend_from_slice(&3_u32.to_be_bytes());
        checked(b"n", &nodes, &[], 1, &body)?;
    }
    Ok(())
}

fn reference_and_export() -> TestResult {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::ForeignReference {
            node: 0,
            creation: 3,
            words: 1..9,
            local_to_context: true,
        },
    ];
    checked(
        b"n\0\0\0\x01\0\0\0\x02",
        &nodes,
        &[],
        1,
        b"Z\0\x02w\x01n\0\0\0\x03\0\0\0\x01\0\0\0\x02",
    )?;
    let nodes = [
        N::Atom {
            bytes: 0..6,
            utf8: true,
        },
        N::Atom {
            bytes: 6..12,
            utf8: true,
        },
        N::ForeignExport {
            module: 0,
            function: 1,
            arity: 1,
        },
    ];
    checked(
        b"erlanglength",
        &nodes,
        &[],
        2,
        b"qw\x06erlangw\x06lengtha\x01",
    )?;
    Ok(())
}

fn local_function() -> TestResult {
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
            id: 0,
            serial: 0,
            creation: 0,
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
    checked(b"mn", &nodes, &[3], 4, &f::local_fun_body())?;
    Ok(())
}
