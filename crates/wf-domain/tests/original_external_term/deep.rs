use super::{I, N, TestResult, fixtures as f};

#[test]
fn external_term_roundtrips_deep_sequences_without_recursive_owners() -> TestResult {
    f::current()?;
    let depth = 2048;
    let mut nodes = vec![N::Integer(7)];
    let mut edges = Vec::new();
    for child in 0..depth {
        edges.push(child);
        nodes.push(N::Tuple {
            children: child..child + 1,
        });
    }
    let prepared = f::healthy(
        I {
            bytes: &[],
            nodes: &nodes,
            edges: &edges,
            root: depth,
        },
        f::limits(),
    )?;
    let mut expected = vec![131];
    for _ in 0..depth {
        expected.extend_from_slice(&[104, 1]);
    }
    expected.extend_from_slice(&[97, 7]);
    assert_eq!(prepared.as_external_term(), expected);
    let decoded = f::decoded(
        prepared.into_external_term(),
        &f::context(),
        f::decode_limits(),
    )?;
    let mut id = decoded.root().ok_or("root")?;
    let mut found = 0;
    while let N::Tuple { children } = id {
        let [child] = decoded.edge_range(children.clone()).ok_or("tuple edges")? else {
            return Err("tuple arity".into());
        };
        id = decoded.node(*child).ok_or("child")?;
        found += 1;
    }
    assert_eq!(found, depth);
    assert!(matches!(id, N::Integer(7)));
    Ok(())
}
