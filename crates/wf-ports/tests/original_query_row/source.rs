//! Complete source cases b0cc11dd93c8b895, d6923153ff84b1b6,
//! ea8f825a3e153721 and7d68296b137453a2. Source async:true, no local
//! setup or flow tag; fixture records and locators preserve the pinned helpers.
use super::{fixtures as f, *};
use sha2::{Digest, Sha256};

#[test]
fn validates_the_bounded_record_before_a_physical_locator_exists() -> TestResult {
    let key = f::key()?;
    assert!(prepare(I::Binary(&key), f::record(), f::limits()).is_ok());
    let large = vec![b't'; 32629];
    let rows = f::replace(f::RECORD, "type", I::Binary(&large));
    let result = prepare(I::Binary(&key), I::Map(&rows), f::limits());
    assert_eq!(
        result.as_ref().err().and_then(E::source_error),
        Some("ERR flow query metadata exceeds 32629 bytes")
    );
    assert!(matches!(
        encode(
            I::Binary(&key),
            I::Map(&rows),
            f::locator(),
            I::Integer(0),
            f::limits()
        ),
        Err(E::MetadataTooLarge)
    ));
    Ok(())
}

#[test]
fn every_admitted_record_fits_with_the_largest_valid_locator() -> TestResult {
    let key = f::key()?;
    assert!(prepare(I::Binary(&key), f::record(), f::limits()).is_ok());
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    assert!(
        encoded
            .map_err(|error| format!("{error:?}"))?
            .encoded()
            .len()
            <= 32768
    );
    Ok(())
}

#[test]
fn keeps_the_persisted_query_row_bytes_stable_while_optimizing_construction() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|error| format!("{error:?}"))?;
    assert_eq!(encoded.encoded().len(), 268);
    assert_eq!(
        hex(&Sha256::digest(encoded.encoded())),
        "3215d2c4b601a25ec373a07fa7a0aee0901533700cf5e4ec815d18db9f871a3b"
    );
    Ok(())
}

#[test]
fn encoding_is_deterministic_across_nested_map_insertion_order() -> TestResult {
    let key = f::key()?;
    let locator = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(9000));
    let first = encode(
        I::Binary(&key),
        f::record(),
        Locator {
            fields: I::Map(&locator),
        },
        I::Integer(9000),
        f::limits(),
    );
    assert!(first.is_ok());
    let attrs = [
        (
            I::Binary(b"labels"),
            I::List(&[I::Binary(b"urgent"), I::Binary(b"finance")]),
        ),
        (I::Binary(b"tier"), I::Binary(b"gold")),
    ];
    let meta = [
        (
            I::Binary(b"queued"),
            I::Map(&[
                (I::Binary(b"score"), I::Float(1.5)),
                (I::Binary(b"worker"), I::Binary(b"worker-1")),
            ]),
        ),
        (
            I::Binary(b"created"),
            I::Map(&[(I::Binary(b"source"), I::Binary(b"api"))]),
        ),
    ];
    let mut second = f::replace(f::RECORD, "attributes", I::Map(&attrs));
    second = f::replace(&second, "state_meta", I::Map(&meta));
    second.reverse();
    let second = encode(
        I::Binary(&key),
        I::Map(&second),
        Locator {
            fields: I::Map(&locator),
        },
        I::Integer(9000),
        f::limits(),
    );
    assert!(matches!((first, second), (Ok(a), Ok(b)) if a.encoded() == b.encoded()));
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}
