use super::{N, TestResult, fixtures as f};

#[test]
fn logical_value_preparation_orders_complete_map_keys_deterministically() -> TestResult {
    mixed_keys()?;
    float_keys()?;
    nested_keys()?;
    foreign_keys()?;
    Ok(())
}

fn mixed_keys() -> TestResult {
    let nodes = [
        N::Integer(7),
        N::FloatBits((-3.0_f64).to_bits()),
        N::Atom {
            bytes: 0..4,
            utf8: true,
        },
        N::EmptyList,
        N::Tuple { children: 0..1 },
        N::Map { entries: 1..1 },
        N::List {
            heads: 1..2,
            tail: 3,
        },
        N::Binary { bytes: 4..5 },
        N::Integer(9),
        N::Map { entries: 2..18 },
    ];
    let mut edges = vec![0, 0];
    for key in [7, 3, 2, 1, 0, 4, 5, 6] {
        edges.extend_from_slice(&[key, 8]);
    }
    let mut body = vec![116, 0, 0, 0, 8];
    // Exact term comparison places every integer before floats, even -3.0.
    for key in [
        b"a\x07".as_slice(),
        b"F\xc0\x08\0\0\0\0\0\0",
        b"w\x04kind",
        b"h\x01a\x07",
        b"t\0\0\0\0",
        b"j",
        b"k\0\x01\x07",
        b"m\0\0\0\x01x",
    ] {
        body.extend_from_slice(key);
        body.extend_from_slice(b"a\x09");
    }
    f::exact(b"kindx", &nodes, &edges, 9, &body)?;
    // Repeat the same map with reversed pair order.
    edges.truncate(2);
    for key in [6, 5, 4, 0, 1, 2, 3, 7] {
        edges.extend_from_slice(&[key, 8]);
    }
    f::exact(b"kindx", &nodes, &edges, 9, &body)?;
    Ok(())
}

fn float_keys() -> TestResult {
    let nodes = [
        N::FloatBits(0.0_f64.to_bits()),
        N::FloatBits((-0.0_f64).to_bits()),
        N::Integer(0),
        N::Integer(7),
        N::Map { entries: 0..6 },
    ];
    let mut body = b"t\0\0\0\x03a\0a\x07".to_vec();
    for bits in [(-0.0_f64).to_bits(), 0.0_f64.to_bits()] {
        body.push(70);
        body.extend_from_slice(&bits.to_be_bytes());
        body.extend_from_slice(b"a\x07");
    }
    f::exact(&[], &nodes, &[0, 3, 1, 3, 2, 3], 4, &body)?;
    Ok(())
}

fn nested_keys() -> TestResult {
    let nodes = [
        N::Integer(1),
        N::Integer(2),
        N::Integer(7),
        N::Integer(0),
        N::Map { entries: 0..2 },
        N::Map { entries: 2..4 },
        N::Map { entries: 4..8 },
    ];
    f::exact(
        &[],
        &nodes,
        &[0, 2, 1, 3, 5, 3, 4, 3],
        6,
        b"t\0\0\0\x02t\0\0\0\x01a\x01a\x07a\0t\0\0\0\x01a\x02a\0a\0",
    )?;
    Ok(())
}

fn foreign_keys() -> TestResult {
    let nodes = foreign_key_nodes();
    let mut edges = vec![4, 4];
    for key in [13, 15, 12, 11, 10, 5, 7, 8, 9, 6, 0, 14, 4] {
        edges.extend_from_slice(&[key, 4]);
    }
    let mut body = b"t\0\0\0\x0d".to_vec();
    for key in [
        b"a\x07".to_vec(),
        b"F\xc0\x08\0\0\0\0\0\0".to_vec(),
        b"w\x01n".to_vec(),
        b"Z\0\x01w\x01n\0\0\0\0\0\0\0\x01".to_vec(),
        f::local_fun_body(),
        b"qw\x06erlangw\x06lengtha\x01".to_vec(),
        b"xw\x01n\0\0\0\0\0\0\0\x01\0\0\0\0".to_vec(),
        b"Xw\x01n\0\0\0\0\0\0\0\0\0\0\0\0".to_vec(),
        b"h\0".to_vec(),
        b"t\0\0\0\0".to_vec(),
        b"j".to_vec(),
        b"k\0\x01\x07".to_vec(),
        b"m\0\0\0\x01x".to_vec(),
    ] {
        body.extend_from_slice(&key);
        body.extend_from_slice(b"a\x07");
    }
    f::exact(b"nmerlanglength\0\0\0\x01x", &nodes, &edges, 16, &body)?;
    Ok(())
}

fn foreign_key_nodes() -> Vec<N> {
    vec![
        N::Atom {
            bytes: 0..1,
            utf8: true,
        },
        N::Atom {
            bytes: 1..2,
            utf8: true,
        },
        N::Atom {
            bytes: 2..8,
            utf8: true,
        },
        N::Atom {
            bytes: 8..14,
            utf8: true,
        },
        N::Integer(7),
        N::ForeignPid {
            node: 0,
            id: 0,
            serial: 0,
            creation: 0,
            local_to_context: false,
        },
        N::ForeignReference {
            node: 0,
            creation: 0,
            words: 14..18,
            local_to_context: false,
        },
        N::ForeignPort {
            node: 0,
            id: 1,
            creation: 0,
            local_to_context: false,
        },
        N::ForeignExport {
            module: 2,
            function: 3,
            arity: 1,
        },
        N::ForeignLocalFun {
            module: 1,
            arity: 1,
            index: 0,
            uniq: [0; 16],
            old_index: 0,
            old_uniq: 7,
            creator: 5,
            free: 0..1,
        },
        N::Tuple { children: 0..0 },
        N::Map { entries: 0..0 },
        N::EmptyList,
        N::Binary { bytes: 18..19 },
        N::FloatBits((-3.0_f64).to_bits()),
        N::List {
            heads: 1..2,
            tail: 12,
        },
        N::Map { entries: 2..28 },
    ]
}
