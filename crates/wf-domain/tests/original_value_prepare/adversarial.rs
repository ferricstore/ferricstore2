//! Additional controls written after implementation, separate from genuine R0.

use wf_domain::value::prepare_original_flow_value_with_init_pid as captured;

use super::{E, I, N, TestResult, fixtures as f, prepare};

#[test]
fn logical_value_preparation_keeps_raw_caps_and_shared_edges_independent() -> TestResult {
    for bytes in [b"".as_slice(), b"x", b"FSV1data", &[b'x'; 100]] {
        let nodes = [N::Binary {
            bytes: 0..bytes.len(),
        }];
        let mut limits = f::limits();
        limits.max_encoded_bytes = bytes.len().max(1);
        let value = prepare(
            I {
                bytes,
                nodes: &nodes,
                edges: &[],
                root: 0,
            },
            limits,
        )?;
        assert_eq!(value.as_stored_bytes(), bytes);
    }
    for (bytes, range, expected) in [
        (b"FSV2plain".as_slice(), 4..9, b"plain".as_slice()),
        (b"xxxFSV2".as_slice(), 3..7, b"FSV2\x01FSV2".as_slice()),
    ] {
        let nodes = [N::Binary { bytes: range }];
        let mut limits = f::limits();
        limits.max_encoded_bytes = expected.len();
        assert_eq!(
            prepare(
                I {
                    bytes,
                    nodes: &nodes,
                    edges: &[],
                    root: 0
                },
                limits
            )?
            .as_stored_bytes(),
            expected
        );
    }
    let nodes = [
        N::Integer(2),
        N::Integer(1),
        N::Integer(9),
        N::Map { entries: 0..4 },
        N::Tuple { children: 0..4 },
        N::Tuple { children: 4..6 },
    ];
    let edges = [0, 2, 1, 2, 3, 4];
    f::exact(
        &[],
        &nodes,
        &edges,
        5,
        b"h\x02t\0\0\0\x02a\x01a\x09a\x02a\x09h\x04a\x02a\x09a\x01a\x09",
    )?;
    assert_eq!(edges, [0, 2, 1, 2, 3, 4]);
    Ok(())
}

#[test]
fn logical_value_preparation_orders_foreign_identity_fields_and_fun_environment() -> TestResult {
    process_order()?;
    port_order()?;
    reference_order()?;
    function_order()?;
    Ok(())
}

fn pair(bytes: &[u8], nodes: &[N], edges: &[usize], first: &[u8], second: &[u8]) -> TestResult {
    let mut expected = b"t\0\0\0\x02".to_vec();
    for key in [first, second] {
        expected.extend_from_slice(key);
        expected.extend_from_slice(b"a\x07");
    }
    f::exact(
        bytes,
        nodes,
        edges,
        nodes.len().checked_sub(1).ok_or("fixture root")?,
        &expected,
    )?;
    Ok(())
}

fn pid_bytes(name: u8, id: u32, serial: u32, creation: u32) -> Vec<u8> {
    let mut bytes = vec![88, 119, 1, name];
    for value in [id, serial, creation] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}

fn process_order() -> TestResult {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::Atom {
            bytes: 1..2,
            utf8: true,
        },
        N::Integer(7),
        N::ForeignPid {
            node: 1,
            id: 0,
            serial: 1,
            creation: 0,
            local_to_context: false,
        },
        N::ForeignPid {
            node: 0,
            id: 999,
            serial: 0,
            creation: 0,
            local_to_context: false,
        },
        N::Map { entries: 0..4 },
    ];
    pair(
        b"na",
        &nodes,
        &[3, 2, 4, 2],
        &pid_bytes(b'n', 999, 0, 0),
        &pid_bytes(b'a', 0, 1, 0),
    )?;
    Ok(())
}

fn port_bytes(name: u8, id: u64) -> Vec<u8> {
    let mut bytes = vec![120, 119, 1, name];
    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes
}

fn port_order() -> TestResult {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::Atom {
            bytes: 1..2,
            utf8: true,
        },
        N::Integer(7),
        N::ForeignPort {
            node: 0,
            id: 0,
            creation: 0,
            local_to_context: false,
        },
        N::ForeignPort {
            node: 1,
            id: u64::MAX,
            creation: 0,
            local_to_context: false,
        },
        N::Map { entries: 0..4 },
    ];
    pair(
        b"na",
        &nodes,
        &[3, 2, 4, 2],
        &port_bytes(b'a', u64::MAX),
        &port_bytes(b'n', 0),
    )?;
    Ok(())
}

fn ref_bytes(words: [u32; 2]) -> Vec<u8> {
    let mut bytes = b"Z\0\x02w\x01n\0\0\0\0".to_vec();
    for word in words {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes
}

fn reference_order() -> TestResult {
    let mut bytes = b"n".to_vec();
    for word in [u32::MAX, 0, 0, 1] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::Integer(7),
        N::ForeignReference {
            node: 0,
            creation: 0,
            words: 1..9,
            local_to_context: false,
        },
        N::ForeignReference {
            node: 0,
            creation: 0,
            words: 9..17,
            local_to_context: true,
        },
        N::Map { entries: 0..4 },
    ];
    pair(
        &bytes,
        &nodes,
        &[3, 1, 2, 1],
        &ref_bytes([u32::MAX, 0]),
        &ref_bytes([0, 1]),
    )?;
    Ok(())
}

fn fun(
    creator: usize,
    free: std::ops::Range<usize>,
    old_uniq: i64,
    arity: u8,
    uniq: [u8; 16],
) -> N {
    N::ForeignLocalFun {
        module: 0,
        arity,
        index: 0,
        uniq,
        old_index: 0,
        old_uniq,
        creator,
        free,
    }
}

fn function_order() -> TestResult {
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
            local_to_context: false,
        },
        N::Integer(9),
        N::Integer(7),
        fun(2, 0..1, 7, 1, [0; 16]),
        fun(2, 1..2, 7, 1, [0; 16]),
        N::Map { entries: 2..6 },
    ];
    let first = f::local_fun_body();
    let mut second = f::local_fun_body();
    *second.last_mut().ok_or("free fixture")? = 9;
    pair(b"mn", &nodes, &[3, 4, 5, 4, 6, 4], &first, &second)?;
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
            local_to_context: false,
        },
        N::ForeignPid {
            node: 1,
            id: 1,
            serial: 0,
            creation: 0,
            local_to_context: false,
        },
        N::Integer(7),
        fun(2, 0..1, 7, 1, [0; 16]),
        fun(3, 1..2, 7, 1, [0; 16]),
        N::Map { entries: 2..6 },
    ];
    assert!(matches!(
        captured(
            I {
                bytes: b"mn",
                nodes: &nodes,
                edges: &[4, 4, 5, 4, 6, 4],
                root: 7
            },
            2,
            f::limits()
        ),
        Err(E::InvalidInput)
    ));
    Ok(())
}

#[test]
fn logical_value_preparation_rejects_inconsistent_function_entries_and_invalid_foreign_shapes() {
    inconsistent_entries();
    invalid_foreign_shapes();
}

fn inconsistent_entries() {
    for (old_uniq, arity, uniq) in [(8, 1, [0; 16]), (7, 2, [0; 16]), (7, 1, [1; 16])] {
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
                local_to_context: false,
            },
            N::Integer(7),
            fun(2, 0..1, 7, 1, [0; 16]),
            fun(2, 0..1, old_uniq, arity, uniq),
            N::Tuple { children: 1..3 },
        ];
        assert!(matches!(
            captured(
                I {
                    bytes: b"mn",
                    nodes: &nodes,
                    edges: &[3, 4, 5],
                    root: 6
                },
                2,
                f::limits()
            ),
            Err(E::InvalidInput)
        ));
    }
}

fn invalid_foreign_shapes() {
    for words in [1..1, 1..25] {
        let nodes = [
            N::Atom {
                bytes: 0..1,
                utf8: true,
            },
            N::ForeignReference {
                node: 0,
                creation: 0,
                words,
                local_to_context: false,
            },
        ];
        assert!(matches!(
            prepare(
                I {
                    bytes: b"n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
                    nodes: &nodes,
                    edges: &[],
                    root: 1
                },
                f::limits()
            ),
            Err(E::InvalidInput)
        ));
    }
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::ForeignExport {
            module: 0,
            function: 0,
            arity: 256,
        },
    ];
    assert!(matches!(
        prepare(
            I {
                bytes: b"n",
                nodes: &nodes,
                edges: &[],
                root: 1
            },
            f::limits()
        ),
        Err(E::InvalidInput)
    ));
    for node in [
        fun(0, 0..0, 7, 1, [0; 16]),
        fun(0, 0..0, 1_i64 << 59, 1, [0; 16]),
    ] {
        let nodes = [
            N::Atom {
                bytes: 0..1,
                utf8: true,
            },
            node,
        ];
        assert!(matches!(
            prepare(
                I {
                    bytes: b"m",
                    nodes: &nodes,
                    edges: &[],
                    root: 1
                },
                f::limits()
            ),
            Err(E::InvalidInput)
        ));
    }
}
