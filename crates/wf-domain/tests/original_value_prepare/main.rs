//! Logical preparation acceptance; literal recipes are independent of the writer.

#![forbid(unsafe_code)]

use wf_domain::value::{
    OriginalValueEncodeError as E, OriginalValueEncodeLimits as L, OriginalValueInput as I,
    OriginalValueNode as N, PreparedOriginalFlowValue as P, prepare_original_flow_value as prepare,
};

mod budgets;
mod fixtures;
mod foreign;
mod maps;
mod scalars;
mod sequences;
mod validation;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn logical_value_preparation_escapes_only_root_binaries() -> TestResult {
    for bytes in [
        b"".as_slice(),
        b"plain",
        b"FSV1\x02old",
        b"FSV2",
        b"FSV2\x02typed",
        b"FSV2\x01raw",
    ] {
        let nodes = [N::Binary {
            bytes: 0..bytes.len(),
        }];
        let prepared = fixtures::prepared(bytes, &nodes, &[], 0)?;
        let mut expected = Vec::new();
        if bytes.starts_with(b"FSV2") {
            expected.extend_from_slice(b"FSV2\x01");
        }
        expected.extend_from_slice(bytes);
        assert_eq!(prepared.as_stored_bytes(), expected);
        if !bytes.is_empty() {
            let nodes = [N::BitString {
                bytes: 0..bytes.len(),
                last_bits: 8,
            }];
            assert_eq!(
                fixtures::prepared(bytes, &nodes, &[], 0)?.as_stored_bytes(),
                expected
            );
        }
        let nodes = [
            N::Binary {
                bytes: 0..bytes.len(),
            },
            N::List {
                heads: 0..0,
                tail: 0,
            },
        ];
        assert_eq!(
            fixtures::prepared(bytes, &nodes, &[], 1)?.as_stored_bytes(),
            expected
        );
    }
    let nodes = [N::Binary { bytes: 0..4 }];
    let input = I {
        bytes: b"FSV2",
        nodes: &nodes,
        edges: &[],
        root: 0,
    };
    let mut limits = fixtures::limits();
    limits.max_encoded_bytes = 8;
    assert!(matches!(prepare(input, limits), Err(E::Limit)));
    limits.max_encoded_bytes = 9;
    assert_eq!(prepare(input, limits)?.as_stored_bytes(), b"FSV2\x01FSV2");
    Ok(())
}

#[test]
fn logical_value_preparation_owns_output_after_inputs_drop() -> TestResult {
    let value = {
        let bytes = b"FSV2\x02data".to_vec();
        let nodes = vec![
            N::Binary {
                bytes: 0..bytes.len(),
            },
            N::Tuple { children: 0..2 },
        ];
        let edges = vec![0, 0];
        fixtures::prepared(&bytes, &nodes, &edges, 1)?
    };
    let mut expected = fixtures::stored(b"h\x02");
    for _ in 0..2 {
        expected.extend_from_slice(b"m\0\0\0\x09FSV2\x02data");
    }
    assert_eq!(value.as_stored_bytes(), expected);
    assert!(value.work_units() > 0);
    assert!(value.temporary_bytes() >= value.as_stored_bytes().len());
    let pointer = value.as_stored_bytes().as_ptr();
    let bytes = value.into_stored_bytes();
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(bytes, expected);
    Ok(())
}
