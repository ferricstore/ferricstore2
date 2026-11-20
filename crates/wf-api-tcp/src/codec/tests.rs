//! Typed value layout and bounds.

use super::Value;
use super::value::DecodeError;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn bytes(value: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    value
        .encode(&mut out)
        .map_err(|_| "value length exceeds u32")?;
    Ok(out)
}

#[test]
fn every_tag_round_trips() -> TestResult {
    let value = Value::Arr(vec![
        Value::Nil,
        Value::Bool(true),
        Value::Bool(false),
        Value::Int(-7),
        Value::Bin(b"ab".to_vec()),
        Value::Map(vec![(b"k".to_vec(), Value::Unsigned(9))]),
        Value::Float(1.5),
    ]);
    assert_eq!(Value::decode_exact(&bytes(&value)?), Ok(value));
    Ok(())
}

#[test]
fn layouts_match_the_native_protocol() -> TestResult {
    assert_eq!(
        bytes(&Value::Bin(b"OK".to_vec()))?,
        [4, 0, 0, 0, 2, b'O', b'K']
    );
    assert_eq!(bytes(&Value::Int(1))?, [3, 0, 0, 0, 0, 0, 0, 0, 1]);
    assert_eq!(bytes(&Value::Map(Vec::new()))?, [6, 0, 0, 0, 0]);
    assert_eq!(
        bytes(&Value::Map(vec![(b"a".to_vec(), Value::Nil)]))?,
        [6, 0, 0, 0, 1, 0, 0, 0, 1, b'a', 0]
    );
    Ok(())
}

#[test]
fn malformed_values_are_refused() {
    for (input, expected) in [
        (&[4, 0, 0, 0, 5, b'a'][..], DecodeError::Truncated),
        (&[9][..], DecodeError::UnknownTag(9)),
        (&[0, 0][..], DecodeError::TrailingBytes),
        (&[5, 0xFF, 0xFF, 0xFF, 0xF0][..], DecodeError::TooManyItems),
        (&[5, 0, 0, 0, 9, 0][..], DecodeError::Truncated),
        (&[6, 0, 0, 0, 2, 0, 0, 0, 0, 0][..], DecodeError::Truncated),
        (&[][..], DecodeError::Truncated),
    ] {
        assert_eq!(Value::decode_exact(input), Err(expected), "{input:?}");
    }
}

#[test]
fn nesting_is_bounded() {
    let nested = |depth: usize| {
        let mut out = Vec::new();
        for _ in 0..depth {
            out.extend_from_slice(&[5, 0, 0, 0, 1]);
        }
        out.push(0);
        out
    };
    assert!(Value::decode_exact(&nested(32)).is_ok());
    assert_eq!(Value::decode_exact(&nested(33)), Err(DecodeError::TooDeep));
}

#[test]
fn integer_and_flag_readers_accept_only_their_types() {
    assert_eq!(Value::Unsigned(5).as_i64(), Some(5));
    assert_eq!(Value::Unsigned(u64::MAX).as_i64(), None);
    assert_eq!(Value::Bin(b"1".to_vec()).as_i64(), None);
    assert_eq!(Value::Bool(true).as_bool(), Some(true));
    assert_eq!(Value::Int(1).as_bool(), None);
}
