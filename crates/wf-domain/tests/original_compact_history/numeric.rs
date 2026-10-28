use super::{C, M, TestResult, fixtures as f};

#[test]
fn compact_history_numeric_presence_matches_original_primitives() -> TestResult {
    let first = f::healthy(&f::empty(), f::limits())?;
    assert_eq!(first.as_bytes(), b"FSH2\x01\x02e\x02\x01\x02s");
    for (number, expected) in [
        (M::Integer(-1), b"0".as_slice()),
        (M::Integer(0), b"0"),
        (M::Integer(126), b"126"),
        (M::Integer(127), b"127"),
        (M::Float(1.5), b"0"),
        (
            M::BigInteger {
                negative: false,
                little_endian: &[1, 0, 0],
            },
            b"1",
        ),
        (
            M::BigInteger {
                negative: true,
                little_endian: &[0, 0],
            },
            b"0",
        ),
    ] {
        let mut snapshot = f::empty();
        snapshot.version = number;
        let bytes = f::healthy(&snapshot, f::limits())?;
        let value = f::decoded(bytes.as_bytes(), C::default(), f::limits())?;
        assert_eq!(f::value(&value, b"version"), Some(expected));
    }
    let mut snapshot = f::empty();
    snapshot.at = M::Integer(1);
    snapshot.numbers[3] = M::BigInteger {
        negative: false,
        little_endian: &[1, 0],
    };
    snapshot.numbers[4] = M::Integer(1);
    let equal = f::healthy(&snapshot, f::limits())?;
    assert_eq!(equal.as_bytes(), b"FSH2\x01\x02e\x02\x02\x02s");
    snapshot.version = M::Integer((1_i128 << 70) - 2);
    let boundary = f::healthy(&snapshot, f::limits())?;
    let value = f::decoded(boundary.as_bytes(), C::default(), f::limits())?;
    assert_eq!(
        f::value(&value, b"version"),
        Some(b"1180591620717411303422".as_slice())
    );
    snapshot.version = M::Integer((1_i128 << 70) - 1);
    let oversized = f::healthy(&snapshot, f::limits())?;
    assert!(
        f::decoded(oversized.as_bytes(), C::default(), f::limits())?
            .fields()
            .is_empty()
    );
    let mut snapshot = f::empty();
    snapshot.numbers[5] = M::Integer(0);
    snapshot.state = Some(b"");
    snapshot.references[0] = Some(b"");
    let zero = f::healthy(&snapshot, f::limits())?;
    assert_eq!(zero.as_bytes(), b"FSH2\x21\x02e\x02\x01\x01\x01");
    let value = f::decoded(zero.as_bytes(), C::default(), f::limits())?;
    assert_eq!(f::value(&value, b"next_run_at_ms"), Some(b"0".as_slice()));
    assert_eq!(f::value(&value, b"lease_deadline_ms"), Some(b"".as_slice()));
    snapshot = f::empty();
    snapshot.numbers[0] = M::Integer(-1);
    let negative = f::healthy(&snapshot, f::limits())?;
    assert_eq!(negative.as_bytes(), b"FSH2\x02\x02e\x02\x01\x02s\0");
    Ok(())
}

#[test]
fn compact_history_arbitrary_magnitudes_match_independent_wire_vectors() -> TestResult {
    let carry128 = [0xff; 16];
    let carry512 = [0xff; 64];
    let padded_zero = [0; 64];
    let mut encoded128 = vec![0x80; 18];
    encoded128.push(4);
    let mut encoded512 = vec![0x80; 73];
    encoded512.push(2);
    for (digits, integer, decoded) in [
        (b"".as_slice(), b"\x01".as_slice(), Some(b"0".as_slice())),
        (padded_zero.as_slice(), b"\x01", Some(b"0")),
        (b"\x7e", b"\x7f", Some(b"126")),
        (b"\x7f", b"\x80\x01", Some(b"127")),
        (b"\x80", b"\x81\x01", Some(b"128")),
        (b"\xff", b"\x80\x02", Some(b"255")),
        (b"\xff\x3f", b"\x80\x80\x01", Some(b"16383")),
        (carry128.as_slice(), encoded128.as_slice(), None),
        (carry512.as_slice(), encoded512.as_slice(), None),
    ] {
        let mut snapshot = f::empty();
        snapshot.version = M::BigInteger {
            negative: false,
            little_endian: digits,
        };
        let encoded = f::healthy(&snapshot, f::limits())?;
        let mut expected = b"FSH2\x01\x02e".to_vec();
        expected.extend_from_slice(integer);
        expected.extend_from_slice(b"\x01\x02s");
        assert_eq!(encoded.as_bytes(), expected);
        let value = f::decoded(encoded.as_bytes(), C::default(), f::limits())?;
        match decoded {
            Some(text) => assert_eq!(f::value(&value, b"version"), Some(text)),
            None => assert!(value.fields().is_empty()),
        }
    }
    let mut snapshot = f::empty();
    snapshot.version = M::BigInteger {
        negative: true,
        little_endian: &carry512,
    };
    let encoded = f::healthy(&snapshot, f::limits())?;
    assert_eq!(encoded.as_bytes(), b"FSH2\x01\x02e\0\x01\x02s");
    let value = f::decoded(encoded.as_bytes(), C::default(), f::limits())?;
    assert_eq!(f::value(&value, b"version"), Some(b"0".as_slice()));
    Ok(())
}

#[test]
fn compact_history_flags_use_exact_nonstrict_numeric_equality() -> TestResult {
    for (number, at, expected) in [
        (
            M::Integer(1),
            M::Float(1.0),
            b"FSH2\x01\x02e\x02\0\x02s".as_slice(),
        ),
        (M::Integer(0), M::Float(-0.0), b"FSH2\x01\x02e\x02\0\x02s"),
        (M::Integer(-1), M::Float(-1.0), b"FSH2\x01\x02e\x02\0\x02s"),
        (
            M::Integer(1),
            M::Float(1.5),
            b"FSH2\x19\x02e\x02\0\x02s\x02\x02",
        ),
    ] {
        let mut snapshot = f::empty();
        snapshot.at = at;
        snapshot.numbers[3] = number;
        snapshot.numbers[4] = number;
        let encoded = f::healthy(&snapshot, f::limits())?;
        assert_eq!(encoded.as_bytes(), expected);
    }
    let mut magnitude = [0; 26];
    magnitude[25] = 1;
    let mut snapshot = f::empty();
    snapshot.at = M::Float(f64::from_bits(1223_u64 << 52));
    snapshot.numbers[3] = M::BigInteger {
        negative: false,
        little_endian: &magnitude,
    };
    snapshot.numbers[4] = snapshot.numbers[3];
    let equal = f::healthy(&snapshot, f::limits())?;
    assert_eq!(equal.as_bytes(), b"FSH2\x01\x02e\x02\0\x02s");
    let mut neighbor = magnitude;
    neighbor[0] = 1;
    snapshot.numbers[3] = M::BigInteger {
        negative: false,
        little_endian: &neighbor,
    };
    let unequal = f::healthy(&snapshot, f::limits())?;
    let mut expected = b"FSH2\x09\x02e\x02\0\x02s\x82".to_vec();
    expected.extend_from_slice(&[0x80; 27]);
    expected.push(0x10);
    assert_eq!(unequal.as_bytes(), expected);
    Ok(())
}
