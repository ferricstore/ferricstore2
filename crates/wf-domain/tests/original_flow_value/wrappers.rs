use super::{C, E, L, N, decode};

#[test]
fn flow_value_preserves_plain_and_escape1_binary_branches() {
    let context = C {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    for (stored, payload) in [
        (b"".as_slice(), b"".as_slice()),
        (b"\xff\0\x01", b"\xff\0\x01"),
        (b"F", b"F"),
        (b"FSV", b"FSV"),
        (b"FSV2", b"FSV2"),
        (b"FSV2\0opaque", b"FSV2\0opaque"),
        (b"FSV2\x03", b"FSV2\x03"),
        (b"FSV2\x01", b""),
        (b"FSV2\x01plain", b"plain"),
        (
            b"FSV2\x01FSV2\x02not-an-external-term",
            b"FSV2\x02not-an-external-term",
        ),
        (b"FSV2\x01FSV2\x01x", b"FSV2\x01x"),
    ] {
        let actual = decode(stored.to_vec(), &context, limits);
        assert!(actual.is_ok(), "{stored:?}: {actual:?}");
        if let Ok(document) = actual {
            assert_eq!(document.encoded(), stored);
            assert!(matches!(document.root(), Some(N::Binary { bytes })
                if document.byte_range(bytes.clone()) == Some(payload)));
        }
    }
}

#[test]
fn flow_value_rejects_compression_trailing_truncation_and_bad_lengths() {
    let context = C {
        safe_atoms: Some(&["ok", "nil"]),
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
    // The compressed body is the original VM's valid compressed term vector.
    let compressed = [
        131, 80, 0, 0, 16, 5, 120, 156, 237, 193, 65, 17, 0, 0, 8, 3, 160, 69, 176, 168, 255, 197,
        183, 135, 7, 108, 50, 41, 0, 0, 0, 240, 222, 1, 39, 39, 128, 231,
    ];
    for body in [
        b"".as_slice(),
        b"a\x01",
        b"\x83",
        b"\x83w\x02ok\0",
        b"\x83h\x02a\x01",
        b"\x83m\0\0\0\x02x",
        b"\x83l\0\0\0\x01a\x01",
        b"\x83t\0\0\0\x01a\x01",
        b"\x83n\x02\0\x01",
        b"\x83m\xff\xff\xff\xff",
        b"\x83\xff",
        compressed.as_slice(),
    ] {
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(body);
        assert!(matches!(
            decode(stored, &context, limits),
            Err(E::InvalidFlowValue)
        ));
    }
}
