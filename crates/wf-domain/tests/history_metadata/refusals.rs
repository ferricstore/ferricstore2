use super::{E, I, N, TestResult, fixtures as f, prepare};

#[test]
fn history_metadata_rejects_invalid_selected_captures() -> TestResult {
    f::ready()?;
    for scalar in [
        N::Binary { bytes: 1..99 },
        N::Atom {
            bytes: 1..2,
            utf8: true,
        },
        N::BigInteger {
            negative: false,
            digits: 1..99,
        },
        N::FloatBits(f64::NAN.to_bits()),
        N::FloatBits(f64::INFINITY.to_bits()),
    ] {
        let nodes = [N::Binary { bytes: 0..1 }, scalar, N::Map { entries: 0..2 }];
        assert!(matches!(
            prepare(
                I {
                    bytes: b"k\xff",
                    nodes: &nodes,
                    edges: &[0, 1],
                    root: 2
                },
                f::limits()
            ),
            Err(E::InvalidInput)
        ));
    }
    let nodes = [
        N::Binary { bytes: 0..1 },
        N::Integer(7),
        N::Map { entries: 0..2 },
    ];
    for (root, edges) in [(99, [0, 1]), (2, [99, 1]), (2, [0, 99])] {
        assert!(matches!(
            prepare(
                I {
                    bytes: b"k",
                    nodes: &nodes,
                    edges: &edges,
                    root
                },
                f::limits()
            ),
            Err(E::InvalidInput)
        ));
    }
    let nodes = [
        N::Binary { bytes: 0..1 },
        N::Integer(7),
        N::Map { entries: 0..1 },
    ];
    assert!(matches!(
        prepare(
            I {
                bytes: b"k",
                nodes: &nodes,
                edges: &[0],
                root: 2
            },
            f::limits()
        ),
        Err(E::InvalidInput)
    ));
    // Unsupported compound bodies and foreign identity details are not inspected.
    let nodes = [
        N::Binary { bytes: 0..1 },
        N::Tuple {
            children: usize::MAX..usize::MAX,
        },
        N::Map { entries: 0..2 },
    ];
    let ignored = f::healthy(
        I {
            bytes: b"k",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        f::limits(),
    )?;
    assert!(ignored.fields().is_empty());
    let nodes = [N::Integer(7), N::Map { entries: 0..2 }];
    let ignored = f::healthy(
        I {
            bytes: &[],
            nodes: &nodes,
            edges: &[0, usize::MAX],
            root: 1,
        },
        f::limits(),
    )?;
    assert!(ignored.fields().is_empty());
    Ok(())
}
