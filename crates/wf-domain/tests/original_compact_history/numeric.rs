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
