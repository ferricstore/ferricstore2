//! Test allocations and independent compressed-vector construction are not SUT work.

use wf_domain::value::{
    OriginalValueEncodeLimits, OriginalValueInput, decode_original_flow_value_compat,
    prepare_original_flow_value,
};

use super::{D, N, TestResult};

// Independent ETF map recipe, compressed with stdlib zlib level9. The expanded
// body excludes version131, per https://www.erlang.org/doc/apps/erts/erl_ext_dist.html.
// This is a valid compressed source payload, not a live VM capture.
pub(super) const COMPRESSED: &[u8] = &[
    131, 80, 0, 0, 20, 16, 120, 218, 237, 196, 193, 9, 192, 32, 0, 4, 176, 43, 56, 66, 71, 212,
    135, 72, 197, 135, 80, 220, 190, 123, 148, 228, 145, 157, 228, 122, 203, 232, 179, 62, 201,
    157, 125, 86, 171, 146, 36, 73, 146, 36, 73, 146, 126, 217, 7, 36, 151, 155, 144,
];

pub(super) fn limits() -> super::super::L {
    super::super::L {
        max_encoded_bytes: 262_144,
        max_nodes: 4_096,
        max_edges: 8_192,
        max_depth: 128,
        max_work_units: 4_194_304,
    }
}

pub(super) fn context() -> super::super::C<'static> {
    super::super::C {
        safe_atoms: Some(&["kind", "old", "true"]),
        active_exports: None,
        local_node: None,
        local_functions: None,
    }
}

pub(super) fn encoded_binary(bytes: &[u8]) -> TestResult<Vec<u8>> {
    prepared(
        bytes,
        &[N::Binary {
            bytes: 0..bytes.len(),
        }],
        &[],
        0,
    )
}

pub(super) fn encoded_map(key: &[u8], value: &[u8], atom: bool) -> TestResult<Vec<u8>> {
    let bytes = [key, value].concat();
    let value_range = key.len()..bytes.len();
    let nodes = [
        N::Atom {
            bytes: 0..key.len(),
            utf8: true,
        },
        if atom {
            N::Atom {
                bytes: value_range,
                utf8: true,
            }
        } else {
            N::Binary { bytes: value_range }
        },
        N::Map { entries: 0..2 },
    ];
    prepared(&bytes, &nodes, &[0, 1], 2)
}

fn prepared(bytes: &[u8], nodes: &[N], edges: &[usize], root: usize) -> TestResult<Vec<u8>> {
    let limits = OriginalValueEncodeLimits {
        max_input_bytes: 65_536,
        max_nodes: 4_096,
        max_edges: 8_192,
        max_depth: 128,
        max_encoded_bytes: 262_144,
        max_work_units: 4_194_304,
        max_temporary_bytes: 4_194_304,
    };
    Ok(prepare_original_flow_value(
        OriginalValueInput {
            bytes,
            nodes,
            edges,
            root,
        },
        limits,
    )?
    .into_stored_bytes())
}

pub(super) fn decoded(bytes: Vec<u8>) -> TestResult<(D, usize)> {
    let result = decode_original_flow_value_compat(bytes, &context(), limits());
    assert!(
        result.is_ok(),
        "healthy source compatibility decode: {:?}",
        result.as_ref().err()
    );
    result.map_err(|error| format!("source compat: {error:?}").into())
}

pub(super) fn binary(document: &D) -> TestResult<&[u8]> {
    let Some(N::Binary { bytes }) = document.root() else {
        return Err("binary root".into());
    };
    document
        .byte_range(bytes.clone())
        .ok_or_else(|| "binary range".into())
}

pub(super) fn map_fields(document: &D) -> TestResult<(&[u8], &[u8])> {
    let Some(N::Map { entries }) = document.root() else {
        return Err("map root".into());
    };
    let ids = document.edge_range(entries.clone()).ok_or("map entries")?;
    if ids.len() != 2 {
        return Err("singleton map".into());
    }
    let Some(N::Atom { bytes: key, .. }) = document.node(ids[0]) else {
        return Err("atom key".into());
    };
    let Some(N::Binary { bytes: value }) = document.node(ids[1]) else {
        return Err("binary value".into());
    };
    Ok((
        document.byte_range(key.clone()).ok_or("key range")?,
        document.byte_range(value.clone()).ok_or("value range")?,
    ))
}

pub(super) fn expected_payload_term() -> Vec<u8> {
    let mut bytes = b"\x83t\0\0\0\x01w\x04kindm\0\0\x14\0".to_vec();
    bytes.extend_from_slice(&b"typed".repeat(1_024));
    bytes
}
