use super::{E, I, L, N, TestResult, fixtures as f, prepare};

#[test]
fn logical_value_preparation_enforces_independent_budgets() -> TestResult {
    let nodes = [N::Binary { bytes: 0..7 }, N::Tuple { children: 0..2 }];
    let edges = [0, 0];
    let input = I {
        bytes: b"payload",
        nodes: &nodes,
        edges: &edges,
        root: 1,
    };
    let value = f::prepared(input.bytes, input.nodes, input.edges, input.root)?;
    for axis in 0..7 {
        let mut short = f::limits();
        *ceiling(&mut short, axis) = match axis {
            0 => 6,
            1..=3 => 1,
            4 => value.as_stored_bytes().len() - 1,
            5 => value.work_units() - 1,
            _ => value.temporary_bytes() - 1,
        };
        assert!(
            matches!(prepare(input, short), Err(E::Limit)),
            "axis {axis}"
        );
        *ceiling(&mut short, axis) = 0;
        assert!(
            matches!(prepare(input, short), Err(E::InvalidLimits)),
            "axis {axis}"
        );
    }
    for axis in [4, 5, 6] {
        let mut exact = f::limits();
        *ceiling(&mut exact, axis) = match axis {
            4 => value.as_stored_bytes().len(),
            5 => value.work_units(),
            _ => value.temporary_bytes(),
        };
        assert_eq!(
            prepare(input, exact)?.as_stored_bytes(),
            value.as_stored_bytes()
        );
    }
    let mut nodes = vec![N::Integer(7)];
    let mut edges = Vec::new();
    for child in 0..28 {
        let start = edges.len();
        edges.extend_from_slice(&[child, child]);
        nodes.push(N::Tuple {
            children: start..start + 2,
        });
    }
    // Compact shared input must not admit exponential serialized expansion.
    let input = I {
        bytes: &[],
        nodes: &nodes,
        edges: &edges,
        root: 28,
    };
    assert!(matches!(prepare(input, f::limits()), Err(E::Limit)));
    Ok(())
}

fn ceiling(limits: &mut L, axis: usize) -> &mut usize {
    match axis {
        0 => &mut limits.max_input_bytes,
        1 => &mut limits.max_nodes,
        2 => &mut limits.max_edges,
        3 => &mut limits.max_depth,
        4 => &mut limits.max_encoded_bytes,
        5 => &mut limits.max_work_units,
        _ => &mut limits.max_temporary_bytes,
    }
}
