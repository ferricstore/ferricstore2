//! Test owners and literal byte recipes are outside production resource claims.

use super::{I, L, N, P, TestResult, prepare};

pub(super) fn limits() -> L {
    L {
        max_input_bytes: 1_048_576,
        max_nodes: 100_000,
        max_edges: 200_000,
        max_depth: 8_192,
        max_encoded_bytes: 4_194_304,
        max_work_units: 134_217_728,
        max_temporary_bytes: 67_108_864,
    }
}

pub(super) fn prepared(bytes: &[u8], nodes: &[N], edges: &[usize], root: usize) -> TestResult<P> {
    let result = prepare(
        I {
            bytes,
            nodes,
            edges,
            root,
        },
        limits(),
    );
    assert!(
        result.is_ok(),
        "healthy logical value preparation: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn stored(body: &[u8]) -> Vec<u8> {
    let mut bytes = b"FSV2\x02\x83".to_vec();
    bytes.extend_from_slice(body);
    bytes
}

pub(super) fn exact(
    bytes: &[u8],
    nodes: &[N],
    edges: &[usize],
    root: usize,
    body: &[u8],
) -> TestResult<P> {
    let value = prepared(bytes, nodes, edges, root)?;
    assert_eq!(value.as_stored_bytes(), stored(body));
    Ok(value)
}

pub(super) fn local_fun_body() -> Vec<u8> {
    // NEW_FUN size54 includes its own four bytes and the one free integer.
    let mut body = vec![112, 0, 0, 0, 54, 1];
    body.extend_from_slice(&[0; 20]);
    body.extend_from_slice(b"\0\0\0\x01w\x01ma\0a\x07Xw\x01n");
    body.extend_from_slice(&[0; 12]);
    body.extend_from_slice(b"a\x07");
    body
}
