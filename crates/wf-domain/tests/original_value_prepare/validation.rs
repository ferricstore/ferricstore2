use super::{E, I, N, TestResult, fixtures as f, prepare};

#[test]
fn logical_value_preparation_rejects_invalid_graphs_and_duplicate_keys() -> TestResult {
    f::exact(&[], &[N::Integer(7)], &[], 0, b"a\x07")?;
    invalid_graphs_and_atoms();
    invalid_numbers_and_shapes();
    duplicate_names();
    duplicate_lists_and_bits();
    duplicate_references();
    Ok(())
}

fn invalid_graphs_and_atoms() {
    for (bytes, nodes, edges, root) in [
        (vec![], vec![], vec![], 0),
        (vec![], vec![N::Integer(7)], vec![], 1),
        (vec![], vec![N::Tuple { children: 0..1 }], vec![0], 0),
        (
            vec![],
            vec![N::Tuple { children: 0..1 }, N::EmptyList],
            vec![1],
            0,
        ),
        (
            vec![],
            vec![N::List {
                heads: 0..0,
                tail: 0,
            }],
            vec![],
            0,
        ),
        (vec![], vec![N::Binary { bytes: 0..1 }], vec![], 0),
        (
            vec![0xff],
            vec![N::Atom {
                bytes: 0..1,
                utf8: true,
            }],
            vec![],
            0,
        ),
        (
            vec![b'a'; 256],
            vec![N::Atom {
                bytes: 0..256,
                utf8: true,
            }],
            vec![],
            0,
        ),
        (vec![], vec![N::FloatBits(f64::NAN.to_bits())], vec![], 0),
        (
            vec![],
            vec![N::FloatBits(f64::INFINITY.to_bits())],
            vec![],
            0,
        ),
    ] {
        refused(&bytes, &nodes, &edges, root);
    }
}

fn invalid_numbers_and_shapes() {
    for (bytes, nodes, edges, root) in [
        (
            vec![0],
            vec![N::BigInteger {
                negative: true,
                digits: 0..1,
            }],
            vec![],
            0,
        ),
        (
            vec![1, 0],
            vec![N::BigInteger {
                negative: false,
                digits: 0..2,
            }],
            vec![],
            0,
        ),
        (
            vec![0],
            vec![N::BitString {
                bytes: 0..1,
                last_bits: 0,
            }],
            vec![],
            0,
        ),
        (
            vec![0],
            vec![N::BitString {
                bytes: 0..1,
                last_bits: 9,
            }],
            vec![],
            0,
        ),
        (
            vec![],
            vec![N::BitString {
                bytes: 0..0,
                last_bits: 1,
            }],
            vec![],
            0,
        ),
        (
            vec![],
            vec![N::Integer(7), N::Map { entries: 0..1 }],
            vec![0],
            1,
        ),
        (
            vec![],
            vec![
                N::Integer(7),
                N::ForeignPid {
                    node: 0,
                    id: 1,
                    serial: 0,
                    creation: 0,
                    local_to_context: false,
                },
            ],
            vec![],
            1,
        ),
    ] {
        refused(&bytes, &nodes, &edges, root);
    }
}

fn refused(bytes: &[u8], nodes: &[N], edges: &[usize], root: usize) {
    let result = prepare(
        I {
            bytes,
            nodes,
            edges,
            root,
        },
        f::limits(),
    );
    assert!(
        matches!(result, Err(E::InvalidInput)),
        "{:?}",
        result.as_ref().err()
    );
}

fn duplicate_names() {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: false,
        },
        N::Atom {
            bytes: 1..3,
            utf8: true,
        },
        N::Integer(7),
        N::Map { entries: 0..4 },
    ];
    refused(b"\xe9\xc3\xa9", &nodes, &[0, 2, 1, 2], 3);
    let nodes = [
        N::Binary { bytes: 0..1 },
        N::BitString {
            bytes: 0..1,
            last_bits: 8,
        },
        N::Integer(7),
        N::Map { entries: 0..4 },
    ];
    refused(b"x", &nodes, &[0, 2, 1, 2], 3);
}

fn duplicate_lists_and_bits() {
    let nodes = [
        N::EmptyList,
        N::Integer(7),
        N::List {
            heads: 0..0,
            tail: 0,
        },
        N::Map { entries: 0..4 },
    ];
    refused(&[], &nodes, &[0, 1, 2, 1], 3);
    let nodes = [
        N::BitString {
            bytes: 0..1,
            last_bits: 3,
        },
        N::BitString {
            bytes: 1..2,
            last_bits: 3,
        },
        N::Integer(7),
        N::Map { entries: 0..4 },
    ];
    refused(&[0xa0, 0xbf], &nodes, &[0, 2, 1, 2], 3);
}

fn duplicate_references() {
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::ForeignReference {
            node: 0,
            creation: 0,
            words: 1..5,
            local_to_context: false,
        },
        N::ForeignReference {
            node: 0,
            creation: 0,
            words: 1..9,
            local_to_context: true,
        },
        N::Integer(7),
        N::Map { entries: 0..4 },
    ];
    refused(b"n\0\0\0\x01\0\0\0\0", &nodes, &[1, 3, 2, 3], 4);
}
