use super::{E, I, L, N, TestResult, fixtures as f, prepare};

#[test]
fn history_metadata_enforces_independent_resource_bounds() -> TestResult {
    f::ready()?;
    let nodes = [
        N::Binary { bytes: 0..5 },
        N::FloatBits(1.5_f64.to_bits()),
        N::Atom {
            bytes: 0..5,
            utf8: true,
        },
        N::Map { entries: 0..4 },
    ];
    let input = I {
        bytes: b"float",
        nodes: &nodes,
        edges: &[2, 1, 0, 1],
        root: 3,
    };
    let base = f::limits();
    for limits in [
        L {
            max_input_bytes: 1,
            ..base
        },
        L {
            max_pairs: 1,
            ..base
        },
        L {
            max_output_bytes: 15,
            ..base
        },
        L {
            max_work_units: 1,
            ..base
        },
        L {
            max_temporary_bytes: 1,
            ..base
        },
    ] {
        assert!(matches!(prepare(input, limits), Err(E::Limit)));
    }
    for limits in [
        L {
            max_input_bytes: 0,
            ..base
        },
        L {
            max_pairs: 0,
            ..base
        },
        L {
            max_output_bytes: 0,
            ..base
        },
        L {
            max_work_units: 0,
            ..base
        },
        L {
            max_temporary_bytes: 0,
            ..base
        },
    ] {
        assert!(matches!(prepare(input, limits), Err(E::InvalidLimits)));
    }
    let exact = f::healthy(
        input,
        L {
            max_output_bytes: 16,
            ..base
        },
    )?;
    assert_eq!(exact.fields().len(), 2);
    let large = [0; 4096];
    assert!(matches!(
        prepare(
            I {
                bytes: &large,
                ..input
            },
            L {
                max_input_bytes: 1024,
                ..base
            }
        ),
        Err(E::Limit)
    ));
    Ok(())
}

#[test]
fn history_metadata_charges_continuous_work_and_temporary_ownership() -> TestResult {
    f::ready()?;
    let nodes = [
        N::Binary { bytes: 0..5 },
        N::FloatBits(1.5_f64.to_bits()),
        N::Atom {
            bytes: 0..5,
            utf8: true,
        },
        N::Map { entries: 0..4 },
    ];
    let input = I {
        bytes: b"float",
        nodes: &nodes,
        edges: &[2, 1, 0, 1],
        root: 3,
    };
    let limits = f::limits();
    let value = f::healthy(input, limits)?;
    let work = value.work_units();
    let temporary = value.temporary_bytes();
    assert!(work > 1 && temporary > 1);
    assert!(
        prepare(
            input,
            L {
                max_work_units: work,
                max_temporary_bytes: temporary,
                ..limits
            }
        )
        .is_ok()
    );
    assert!(matches!(
        prepare(
            input,
            L {
                max_work_units: work - 1,
                ..limits
            }
        ),
        Err(E::Limit)
    ));
    assert!(matches!(
        prepare(
            input,
            L {
                max_temporary_bytes: temporary - 1,
                ..limits
            }
        ),
        Err(E::Limit)
    ));
    let nodes = [
        N::Binary { bytes: 0..5 },
        N::FloatBits(1.5_f64.to_bits()),
        N::Map { entries: 0..2 },
    ];
    let single = f::healthy(
        I {
            bytes: b"float",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        limits,
    )?;
    assert!(single.work_units() < work);
    assert!(single.temporary_bytes() < temporary);
    Ok(())
}
