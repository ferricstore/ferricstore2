use super::{C, D, DL, I, L, N, P, TestResult, decode, prepare};

pub(super) fn limits() -> L {
    L {
        max_input_bytes: 1_048_576,
        max_nodes: 100_000,
        max_edges: 200_000,
        max_depth: 8192,
        max_encoded_bytes: 4_194_304,
        max_work_units: 134_217_728,
        max_temporary_bytes: 67_108_864,
    }
}

pub(super) fn decode_limits() -> DL {
    DL {
        max_encoded_bytes: 4_194_304,
        max_nodes: 100_000,
        max_edges: 200_000,
        max_depth: 8192,
        max_work_units: 134_217_728,
    }
}

pub(super) fn context() -> C<'static> {
    C {
        safe_atoms: Some(&["current", "value", "n", "m", "nonode@nohost"]),
        active_exports: None,
        local_node: None,
        local_functions: None,
    }
}

pub(super) fn healthy(input: I<'_>, limits: L) -> TestResult<P> {
    let value = prepare(input, limits);
    assert!(
        value.is_ok(),
        "healthy bare external-term preparation: {:?}",
        value.as_ref().err()
    );
    Ok(value?)
}

pub(super) fn decoded(bytes: Vec<u8>, context: &C<'_>, limits: DL) -> TestResult<D> {
    let value = decode(bytes, context, limits);
    assert!(
        value.is_ok(),
        "healthy bare external-term decode: {:?}",
        value.as_ref().err()
    );
    Ok(value.map_err(|error| format!("bare external-term decode: {error:?}"))?)
}

pub(super) fn current_nodes() -> [N; 5] {
    [
        N::Atom {
            bytes: 0..7,
            utf8: true,
        },
        N::Atom {
            bytes: 7..12,
            utf8: true,
        },
        N::Binary { bytes: 12..19 },
        N::Map { entries: 0..2 },
        N::Tuple { children: 2..4 },
    ]
}

pub(super) fn current() -> TestResult<P> {
    let nodes = current_nodes();
    healthy(
        I {
            bytes: b"currentvaluepayload",
            nodes: &nodes,
            edges: &[1, 2, 0, 3],
            root: 4,
        },
        limits(),
    )
}

pub(super) fn atom(document: &D, id: usize) -> TestResult<&[u8]> {
    let Some(N::Atom { bytes, .. }) = document.node(id) else {
        return Err("atom class".into());
    };
    Ok(document.byte_range(bytes.clone()).ok_or("atom bytes")?)
}

pub(super) fn binary(document: &D, id: usize) -> TestResult<&[u8]> {
    let Some(N::Binary { bytes }) = document.node(id) else {
        return Err("binary class".into());
    };
    Ok(document.byte_range(bytes.clone()).ok_or("binary bytes")?)
}

pub(super) fn source_shape(document: &D) -> TestResult<(&[u8], &[u8], &[u8])> {
    let Some(N::Tuple { children }) = document.root() else {
        return Err("tuple root".into());
    };
    let [first, map] = document.edge_range(children.clone()).ok_or("tuple edges")? else {
        return Err("tuple arity".into());
    };
    let Some(N::Map { entries }) = document.node(*map) else {
        return Err("map child".into());
    };
    let [key, value] = document.edge_range(entries.clone()).ok_or("map edges")? else {
        return Err("map arity".into());
    };
    Ok((
        atom(document, *first)?,
        atom(document, *key)?,
        binary(document, *value)?,
    ))
}

pub(super) fn compressed() -> Vec<u8> {
    vec![
        0x83, 0x50, 0x00, 0x00, 0x30, 0x10, 0x78, 0xda, 0xed, 0xc6, 0xc1, 0x09, 0x80, 0x30, 0x10,
        0x00, 0xb0, 0xe2, 0xc7, 0x75, 0x9c, 0xc9, 0x72, 0x50, 0xc1, 0xaa, 0x5c, 0x15, 0xd7, 0x77,
        0x0d, 0x1f, 0xc9, 0x2b, 0x6d, 0x7a, 0xe7, 0xfa, 0x64, 0xc6, 0x71, 0xf7, 0x52, 0x96, 0x52,
        0xcf, 0x7e, 0x65, 0x8c, 0xb1, 0xad, 0x7b, 0xb8, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb,
        0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb,
        0xfb, 0x3f, 0xff, 0x01, 0x0f, 0x1e, 0x25, 0xb5,
    ]
}
