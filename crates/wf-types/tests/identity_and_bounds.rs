//! Binary storage identity and separate composed-key admission assertions.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use wf_types::{
    bounds::{MAX_API_KEY_BYTES, MAX_RECORD_ID_BYTES, checked_api_key_size, validate_api_key_size},
    error::ValueError,
    identity::RecordId,
};

#[test]
fn empty_record_id_is_rejected_with_the_source_api_message() {
    let error = ValueError::EmptyRecordId;
    assert_eq!(RecordId::new(Vec::new()), Err(error.clone()));
    assert_eq!(error.to_string(), "ERR flow id must be a non-empty string");
}

#[test]
fn storage_identity_ceiling_is_exactly_65535_bytes() -> Result<(), ValueError> {
    assert_eq!(MAX_RECORD_ID_BYTES, 65_535);
    for length in [1, 65_534, 65_535] {
        let bytes = vec![0xff; length];
        let id = RecordId::new(bytes.clone())?;
        assert_eq!(id.as_bytes(), bytes);
    }
    assert_eq!(
        RecordId::new(vec![0; 65_536]),
        Err(ValueError::RecordIdTooLong { bytes: 65_536 })
    );
    Ok(())
}

#[test]
fn record_ids_preserve_all_byte_values_without_utf8_or_delimiter_rules() -> Result<(), ValueError> {
    let bytes: Vec<u8> = (u8::MIN..=u8::MAX).collect();
    let id = RecordId::new(bytes.clone())?;
    assert_eq!(id.as_bytes(), bytes);
    for binary in [b"\0".as_slice(), b"\xff", b"a:b", b"{f}:s:", b"\xc0\x80"] {
        assert_eq!(RecordId::new(binary.to_vec())?.as_bytes(), binary);
    }
    Ok(())
}

#[test]
fn record_id_comparison_and_cloning_preserve_binary_identity() -> Result<(), ValueError> {
    let mut identities = BTreeSet::new();
    for byte in u8::MIN..=u8::MAX {
        let id = RecordId::new(vec![byte])?;
        assert_eq!(id.clone(), id);
        assert!(identities.insert(id));
    }
    assert_eq!(identities.len(), 256);
    for (id, byte) in identities.iter().zip(u8::MIN..=u8::MAX) {
        assert_eq!(id.as_bytes(), [byte]);
    }
    Ok(())
}

#[test]
fn valid_storage_id_can_exceed_the_composed_api_key_limit() -> Result<(), ValueError> {
    let id = RecordId::new(vec![b'i'; 65_535])?;
    // Already encoded routing tag supplied by an adapter; this test does not
    // implement the source CRC32/SHA-256 routing or sealed storage scope.
    let prefix = b"f:{fa:0}:s:";
    let mut key = prefix.to_vec();
    key.extend_from_slice(id.as_bytes());
    assert_eq!(
        validate_api_key_size(&key),
        Err(ValueError::ApiKeyTooLong { bytes: key.len() })
    );
    assert_eq!(
        checked_api_key_size(&[prefix.len(), id.as_bytes().len()]),
        Err(ValueError::ApiKeyTooLong { bytes: key.len() })
    );
    Ok(())
}

#[test]
fn composed_api_key_bound_includes_every_prefix_and_separator_byte() -> Result<(), ValueError> {
    assert_eq!(MAX_API_KEY_BYTES, 65_535);
    let prefix = b"f:{fa:0}:s:";
    let bytes = vec![0xff; 65_535 - prefix.len()];
    let id = RecordId::new(bytes)?;
    let mut key = prefix.to_vec();
    key.extend_from_slice(id.as_bytes());
    assert_eq!(key.len(), 65_535);
    validate_api_key_size(&key)?;
    assert_eq!(
        checked_api_key_size(&[prefix.len(), id.as_bytes().len()])?,
        65_535
    );

    key.push(0);
    let error = ValueError::ApiKeyTooLong { bytes: 65_536 };
    assert_eq!(validate_api_key_size(&key), Err(error.clone()));
    assert_eq!(error.to_string(), "ERR key too large (max 65535 bytes)");
    // Source key-size validation alone does not impose a nonempty-key rule.
    validate_api_key_size(b"")?;
    assert_eq!(checked_api_key_size(&[])?, 0);
    Ok(())
}

#[test]
fn composed_key_size_arithmetic_rejects_overflow_before_allocation() {
    assert_eq!(
        checked_api_key_size(&[usize::MAX, 1]),
        Err(ValueError::LengthOverflow)
    );
    assert_eq!(
        checked_api_key_size(&[1, usize::MAX]),
        Err(ValueError::LengthOverflow)
    );
    assert_eq!(
        checked_api_key_size(&[65_535, 1]),
        Err(ValueError::ApiKeyTooLong { bytes: 65_536 })
    );
}
