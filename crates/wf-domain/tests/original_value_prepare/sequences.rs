use super::{N, TestResult, fixtures as f};

#[test]
fn logical_value_preparation_encodes_lists_and_tuple_boundaries() -> TestResult {
    f::exact(&[], &[N::Tuple { children: 0..0 }], &[], 0, b"h\0")?;
    for length in [255, 256] {
        let nodes = [
            N::Integer(7),
            N::Tuple {
                children: 0..length,
            },
        ];
        let mut body = if length == 255 {
            vec![104, 255]
        } else {
            vec![105, 0, 0, 1, 0]
        };
        body.extend_from_slice(&b"a\x07".repeat(length));
        f::exact(&[], &nodes, &vec![0; length], 1, &body)?;
    }
    let nodes = [
        N::Integer(65),
        N::Integer(66),
        N::Integer(67),
        N::EmptyList,
        N::List {
            heads: 0..3,
            tail: 3,
        },
    ];
    f::exact(&[], &nodes, &[0, 1, 2], 4, b"k\0\x03ABC")?;
    let nodes = [
        N::Integer(65),
        N::Integer(66),
        N::Integer(67),
        N::EmptyList,
        N::List {
            heads: 0..2,
            tail: 3,
        },
        N::List {
            heads: 2..3,
            tail: 4,
        },
    ];
    f::exact(&[], &nodes, &[1, 2, 0], 5, b"k\0\x03ABC")?;
    f::exact(
        &[],
        &[
            N::Integer(1),
            N::Integer(7),
            N::List {
                heads: 0..1,
                tail: 1,
            },
        ],
        &[0],
        2,
        b"l\0\0\0\x01a\x01a\x07",
    )?;
    for length in [65_535_usize, 65_536] {
        let nodes = [
            N::Integer(65),
            N::EmptyList,
            N::List {
                heads: 0..length,
                tail: 1,
            },
        ];
        let body = if length == 65_535 {
            let mut body = vec![107, 255, 255];
            body.extend_from_slice(&vec![65; length]);
            body
        } else {
            let mut body = vec![108, 0, 1, 0, 0];
            body.extend_from_slice(&b"aA".repeat(length));
            body.push(106);
            body
        };
        f::exact(&[], &nodes, &vec![0; length], 2, &body)?;
    }
    let depth = 4_096;
    let mut nodes = vec![N::Integer(7)];
    let mut edges = Vec::new();
    for child in 0..depth {
        let start = edges.len();
        edges.push(child);
        nodes.push(N::Tuple {
            children: start..start + 1,
        });
    }
    let mut body = b"h\x01".repeat(depth);
    body.extend_from_slice(b"a\x07");
    f::exact(&[], &nodes, &edges, depth, &body)?;
    Ok(())
}
