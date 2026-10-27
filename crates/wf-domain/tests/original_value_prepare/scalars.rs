use super::{N, TestResult, fixtures as f};

#[test]
fn logical_value_preparation_encodes_canonical_scalars() -> TestResult {
    for (value, body) in [
        (0, b"a\0".as_slice()),
        (255, b"a\xff"),
        (256, b"b\0\0\x01\0"),
        (-1, b"b\xff\xff\xff\xff"),
        (i64::from(i32::MIN), b"b\x80\0\0\0"),
        (i64::from(i32::MAX), b"b\x7f\xff\xff\xff"),
        (i64::MAX, b"n\x08\0\xff\xff\xff\xff\xff\xff\xff\x7f"),
        (i64::MIN, b"n\x08\x01\0\0\0\0\0\0\0\x80"),
    ] {
        f::exact(&[], &[N::Integer(value)], &[], 0, body)?;
    }
    for negative in [false, true] {
        let magnitude = [0, 0, 0, 0, 0, 0, 0, 0, 1];
        let mut body = vec![110, 9, u8::from(negative)];
        body.extend_from_slice(&magnitude);
        f::exact(
            &magnitude,
            &[N::BigInteger {
                negative,
                digits: 0..9,
            }],
            &[],
            0,
            &body,
        )?;
    }
    integer_width_boundaries()?;
    for value in [-0.0_f64, 0.0, -1.5, f64::MIN_POSITIVE, f64::MAX] {
        let bits = value.to_bits();
        let mut body = vec![70];
        body.extend_from_slice(&bits.to_be_bytes());
        f::exact(&[], &[N::FloatBits(bits)], &[], 0, &body)?;
    }
    for (bytes, utf8, expected) in [
        (b"".as_slice(), true, b"w\0".as_slice()),
        (b"kind", true, b"w\x04kind"),
        (b"\xe9", false, b"w\x02\xc3\xa9"),
        ("λ".as_bytes(), true, b"w\x02\xce\xbb"),
    ] {
        f::exact(
            bytes,
            &[N::Atom {
                bytes: 0..bytes.len(),
                utf8,
            }],
            &[],
            0,
            expected,
        )?;
    }
    let name = "λ".repeat(128);
    let mut body = vec![118, 1, 0];
    body.extend_from_slice(name.as_bytes());
    f::exact(
        name.as_bytes(),
        &[N::Atom {
            bytes: 0..256,
            utf8: true,
        }],
        &[],
        0,
        &body,
    )?;
    f::exact(
        &[0xbf],
        &[N::BitString {
            bytes: 0..1,
            last_bits: 3,
        }],
        &[],
        0,
        &[77, 0, 0, 0, 1, 3, 0xa0],
    )?;
    f::exact(&[], &[N::EmptyList], &[], 0, b"j")?;
    Ok(())
}

fn integer_width_boundaries() -> TestResult {
    for length in [255_usize, 256] {
        let mut magnitude = vec![0; length];
        magnitude[length - 1] = 1;
        let mut body = if length == 255 {
            vec![110, 255, 0]
        } else {
            vec![111, 0, 0, 1, 0, 0]
        };
        body.extend_from_slice(&magnitude);
        f::exact(
            &magnitude,
            &[N::BigInteger {
                negative: false,
                digits: 0..length,
            }],
            &[],
            0,
            &body,
        )?;
    }
    Ok(())
}
