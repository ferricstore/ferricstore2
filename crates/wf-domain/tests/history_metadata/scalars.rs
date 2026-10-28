use super::{I, N, TestResult, fixtures as f};

#[test]
fn history_metadata_converts_integer_and_atom_ranges() -> TestResult {
    f::ready()?;
    for (value, expected) in [
        (i64::MIN, "-9223372036854775808"),
        (0, "0"),
        (i64::MAX, "9223372036854775807"),
    ] {
        assert_eq!(f::rendered(b"k", N::Integer(value))?, expected.as_bytes());
    }
    for (negative, expected) in [
        (false, "18446744073709551616"),
        (true, "-18446744073709551616"),
    ] {
        let mut bytes = b"k".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(
            f::rendered(
                &bytes,
                N::BigInteger {
                    negative,
                    digits: 1..10
                }
            )?,
            expected.as_bytes()
        );
    }
    let mut bytes = b"k".to_vec();
    bytes.extend_from_slice(&[255; 64]);
    assert_eq!(f::rendered(&bytes,N::BigInteger { negative:false,digits:1..65 })?,b"13407807929942597099574024998205846127479365820592393377723561443721764030073546976801874298166903427690031858186486050853753882811946569946433649006084095");
    for (bytes, range, utf8, expected) in [
        (b"knil".as_slice(), 1..4, true, b"".as_slice()),
        (b"kfalse", 1..6, true, b"false"),
        (b"k\xe9", 1..2, false, b"\xc3\xa9"),
        ("kλ".as_bytes(), 1..3, true, "λ".as_bytes()),
    ] {
        assert_eq!(
            f::rendered(bytes, N::Atom { bytes: range, utf8 })?,
            expected
        );
    }
    let nodes = [
        N::Atom {
            bytes: 0..1,
            utf8: false,
        },
        N::Integer(7),
        N::Map { entries: 0..2 },
    ];
    let value = f::healthy(
        I {
            bytes: b"\xe9",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        f::limits(),
    )?;
    assert_eq!(value.fields()[0].key.as_ref(), b"\xc3\xa9");
    assert_eq!(
        f::rendered(
            b"k\0\xff",
            N::BitString {
                bytes: 1..3,
                last_bits: 8
            }
        )?,
        b"\0\xff"
    );
    Ok(())
}

#[test]
fn history_metadata_formats_short_finite_floats() -> TestResult {
    f::ready()?;
    for (value, expected) in [
        (0.0_f64, "0.0"),
        (-0.0, "-0.0"),
        (7.0, "7.0"),
        (7.12, "7.12"),
        (0.1 + 0.2, "0.30000000000000004"),
        (100.0, "100.0"),
        (1000.0, "1.0e3"),
        (0.0001, "0.0001"),
        (0.00001, "1.0e-5"),
        (9_007_199_254_740_991.0, "9007199254740991.0"),
        (9_007_199_254_740_992.0, "9.007199254740992e15"),
        (-9_007_199_254_740_992.0, "-9.007199254740992e15"),
        (f64::MAX, "1.7976931348623157e308"),
        (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
        (f64::from_bits(1), "5.0e-324"),
    ] {
        assert_eq!(
            f::rendered(b"k", N::FloatBits(value.to_bits()))?,
            expected.as_bytes()
        );
    }
    let mut bits = 0x0123_4567_89ab_cdef_u64;
    for _ in 0..2048 {
        bits = bits.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        if f64::from_bits(bits).is_finite() {
            let text = f::rendered(b"k", N::FloatBits(bits))?;
            assert_eq!(std::str::from_utf8(&text)?.parse::<f64>()?.to_bits(), bits);
        }
    }
    Ok(())
}

#[test]
fn history_metadata_uses_even_shortest_float_ties() -> TestResult {
    f::ready()?;
    for (bits, expected) in [
        (0xc2eb_754f_8947_92c4, "-241525276163222.12"),
        (0x4313_4561_cab8_089d, "1356077718176295.2"),
        (0x431e_2a11_9f8f_b7d1, "2122626120281588.2"),
        (0x4303_e147_1692_a07a, "699464999392271.2"),
        (0x430d_31bd_0d4f_caea, "1027182795815261.2"),
        (0x431c_4a88_62dc_d185, "1990812245636193.2"),
        (0x42d9_6792_08ce_8dc8, "111730489506359.12"),
    ] {
        assert_eq!(f::rendered(b"k", N::FloatBits(bits))?, expected.as_bytes());
    }
    Ok(())
}

#[test]
fn history_metadata_respects_shortest_float_power_intervals() -> TestResult {
    f::ready()?;
    for (bits, expected) in [
        (0x3e70_0000_0000_0000, "5.960464477539063e-8"),
        (0xbe70_0000_0000_0000, "-5.960464477539063e-8"),
        (0x3e60_0000_0000_0000, "2.9802322387695312e-8"),
    ] {
        assert_eq!(f::rendered(b"k", N::FloatBits(bits))?, expected.as_bytes());
    }
    Ok(())
}
