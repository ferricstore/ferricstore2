use super::{C, E, L, N, decode};

#[test]
fn flow_value_bigints_preserve_sign_truthiness_padding_and_zero() {
    let context = C {
        safe_atoms: Some(&[]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: Some(&[]),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    for (body, expected) in [
        (b"\x83a\x01".as_slice(), 1_i64),
        (b"\x83b\x80\0\0\0", -2_147_483_648),
        (b"\x83b\x7f\xff\xff\xff", 2_147_483_647),
        (b"\x83n\x01\x02\x01", -1),
        (b"\x83n\x01\xff\x01", -1),
        (b"\x83o\0\0\0\x01\x02\x01", -1),
        (b"\x83n\0\x02", 0),
        (b"\x83n\x02\xff\0\0", 0),
        (b"\x83n\x08\0\xff\xff\xff\xff\xff\xff\xff\x7f", i64::MAX),
        (b"\x83n\x08\x01\0\0\0\0\0\0\0\x80", i64::MIN),
    ] {
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(body);
        let actual = decode(stored, &context, limits);
        assert!(actual.is_ok(), "{body:?}: {actual:?}");
        if let Ok(document) = actual {
            assert!(matches!(document.root(), Some(N::Integer(value)) if *value == expected));
        }
    }
    let wide = [1_u8; 20];
    for sign in [0, 2, 255] {
        let mut stored = b"FSV2\x02\x83n\x16".to_vec();
        stored.push(sign);
        stored.extend_from_slice(&wide);
        stored.extend_from_slice(&[0, 0]);
        let actual = decode(stored, &context, limits);
        assert!(actual.is_ok(), "sign {sign}: {actual:?}");
        if let Ok(document) = actual {
            assert!(
                matches!(document.root(), Some(N::BigInteger { negative, digits })
                if *negative == (sign != 0)
                    && document.byte_range(digits.clone()) == Some(wide.as_slice()))
            );
        }
    }
    assert_eq!(wide, [1; 20]);
}

#[test]
fn flow_value_float70_and_float99_preserve_bits_and_reject_nonfinite() {
    let context = C {
        safe_atoms: Some(&[]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: Some(&[]),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    let mut cases = Vec::new();
    for bits in [0_u64, 0x8000_0000_0000_0000, 0x3ff8_0000_0000_0000, 1] {
        let mut body = vec![131, 70];
        body.extend_from_slice(&bits.to_be_bytes());
        cases.push((body, bits));
    }
    for (text, bits) in [
        (b"1.0".as_slice(), 0x3ff0_0000_0000_0000),
        (b"+1.0", 0x3ff0_0000_0000_0000),
        (b"-0.0", 0x8000_0000_0000_0000),
        (b"1.0e+0", 0x3ff0_0000_0000_0000),
        (b"1.0e-9999", 0),
        (b"1,5", 0x3ff8_0000_0000_0000),
        (b"1.0\0ignored", 0x3ff0_0000_0000_0000),
        (b"1.0\0\xff", 0x3ff0_0000_0000_0000),
        (b"1.0000000000000000000000000000", 0x3ff0_0000_0000_0000),
    ] {
        let mut body = vec![131, 99];
        body.extend_from_slice(text);
        body.resize(33, 0);
        cases.push((body, bits));
    }
    for (body, expected_bits) in cases {
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(&body);
        let actual = decode(stored, &context, limits);
        assert!(actual.is_ok(), "{body:?}: {actual:?}");
        if let Ok(document) = actual {
            assert!(matches!(document.root(), Some(N::FloatBits(bits)) if *bits == expected_bits));
        }
    }
    let mut invalid = Vec::new();
    for text in [
        "1", ".5", "1.", " 1.0", "1.0 ", "1.0junk", "1.0e", "1.0e9999", "nan", "inf", "0x1.0p0",
        "+.5", "-.5", "-1e0",
    ] {
        let mut body = vec![131, 99];
        body.extend_from_slice(text.as_bytes());
        body.resize(33, 0);
        invalid.push(body);
    }
    for bits in [
        0x7ff0_0000_0000_0000_u64,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
    ] {
        let mut body = vec![131, 70];
        body.extend_from_slice(&bits.to_be_bytes());
        invalid.push(body);
    }
    // The pinned VM rejects a full field without NUL, also before a tuple child.
    let mut no_nul = vec![131, 99, b'1', b'.'];
    no_nul.resize(33, b'0');
    let mut nested = vec![131, 104, 2];
    nested.extend_from_slice(&no_nul[1..]);
    nested.extend_from_slice(&[97, 7]);
    invalid.extend([no_nul, nested]);
    for body in invalid {
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(&body);
        assert!(matches!(
            decode(stored, &context, limits),
            Err(E::InvalidFlowValue)
        ));
    }
}
