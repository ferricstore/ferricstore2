use super::{E, parts, stored_len};

#[test]
fn logical_binary_parts_preserve_plain_and_escape_fsv2_prefix_once() {
    for (input, expected, prefix, size) in [
        (b"".as_slice(), b"".as_slice(), b"".as_slice(), 0),
        (b"\xff\0", b"\xff\0", b"", 2),
        (b"FSV", b"FSV", b"", 3),
        (b"FSV2", b"FSV2\x01FSV2", b"FSV2\x01", 9),
        (b"FSV2\0\xff", b"FSV2\x01FSV2\0\xff", b"FSV2\x01", 11),
        (b"FSV2\x01x", b"FSV2\x01FSV2\x01x", b"FSV2\x01", 11),
        (
            b"FSV2\x02\x83a\x01",
            b"FSV2\x01FSV2\x02\x83a\x01",
            b"FSV2\x01",
            13,
        ),
        (b"FSV2\xff", b"FSV2\x01FSV2\xff", b"FSV2\x01", 10),
        (
            b"FSV2\x01FSV2\x02not-an-external-term",
            b"FSV2\x01FSV2\x01FSV2\x02not-an-external-term",
            b"FSV2\x01",
            35,
        ),
    ] {
        assert_eq!(expected.len(), size);
        let actual = parts(input, size);
        assert!(actual.is_ok(), "{input:?}: {actual:?}");
        if let Ok(spans) = actual {
            assert_eq!(spans, [prefix, input]);
            assert_eq!(spans.concat(), expected);
            assert!(matches!(stored_len(input, size), Ok(length) if length == size));
        }
    }
}

#[test]
fn logical_binary_stored_length_and_zero_cap_boundaries_are_exact() {
    for (input, size) in [
        (b"".as_slice(), 0),
        (b"\xff\0", 2),
        (b"FSV", 3),
        (b"FSV2", 9),
        (b"FSV2\0\xff", 11),
        (b"FSV2\x01x", 11),
        (b"FSV2\x02\x83a\x01", 13),
        (b"FSV2\xff", 10),
        (b"FSV2\x01FSV2\x02not-an-external-term", 35),
    ] {
        assert!(matches!(stored_len(input, size), Ok(length) if length == size));
        assert!(parts(input, size).is_ok());
        if size > 0 {
            assert!(matches!(stored_len(input, size - 1), Err(E::Limit)));
            assert!(matches!(parts(input, size - 1), Err(E::Limit)));
        }
    }
    let empty = parts(b"", 0);
    assert!(empty.is_ok());
    if let Ok(spans) = empty {
        assert_eq!(spans, [b"".as_slice(), b"".as_slice()]);
    }
    // Logical binaries have no reference/Router65535 limit.
    let mut large = vec![b'x'; 65_536];
    assert!(matches!(stored_len(&large, 65_536), Ok(65_536)));
    assert!(parts(&large, 65_536).is_ok());
    large[..4].copy_from_slice(b"FSV2");
    assert!(matches!(stored_len(&large, 65_541), Ok(65_541)));
    assert!(parts(&large, 65_541).is_ok());
    assert!(matches!(stored_len(&large, 65_540), Err(E::Limit)));
    assert!(matches!(parts(&large, 65_540), Err(E::Limit)));
    assert_eq!(large.len(), 65_536);
}

#[test]
fn logical_binary_parts_borrow_original_payload_backing() {
    // This backing is fixture-owned and deliberately larger than either view.
    let mut backing = [b'!'; 128];
    backing[11..19].copy_from_slice(b"FSV2\x02\x83a\x01");
    backing[64..69].copy_from_slice(b"\xff\0:\0\x01");
    for (start, end, prefix, payload, maximum) in [
        (
            11,
            19,
            b"FSV2\x01".as_slice(),
            b"FSV2\x02\x83a\x01".as_slice(),
            13,
        ),
        (64, 69, b"", b"\xff\0:\0\x01", 5),
    ] {
        let actual = {
            let descriptor = &backing[start..end];
            parts(descriptor, maximum)
        };
        assert_ne!(payload, b"");
        assert!(actual.is_ok(), "{actual:?}");
        if let Ok(spans) = actual {
            assert_eq!(spans[0], prefix);
            assert_eq!(spans[1], payload);
            assert_eq!(spans[1].len(), end - start);
            assert!(std::ptr::eq(spans[1], &raw const backing[start..end]));
            assert!(std::ptr::eq(
                spans[1].as_ptr(),
                backing.as_ptr().wrapping_add(start)
            ));
        }
    }
    assert_eq!(&backing[..11], &[b'!'; 11]);
    assert_eq!(&backing[11..19], b"FSV2\x02\x83a\x01");
    assert_eq!(&backing[64..69], b"\xff\0:\0\x01");
    assert_eq!(&backing[69..], &[b'!'; 59]);
}
