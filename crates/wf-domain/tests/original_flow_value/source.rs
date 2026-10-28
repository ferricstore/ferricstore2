//! Exact pinned FlowCodecTest451/469 observations, through the real codec.

use super::{D, E, N};

mod fixtures;
mod limits;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn source_value_codec_preserves_raw_escape_and_typed_roundtrips() -> TestResult {
    let plain = fixtures::encoded_binary(b"payload-bytes")?;
    assert_eq!(plain, b"payload-bytes");
    let (decoded, _) = fixtures::decoded(plain)?;
    assert_eq!(fixtures::binary(&decoded)?, b"payload-bytes");
    let old_term = fixtures::encoded_map(b"old", b"true", true)?;
    let mut old_magic = b"FSV1".to_vec();
    old_magic.extend_from_slice(old_term.get(5..).ok_or("typed suffix")?);
    let old_encoded = fixtures::encoded_binary(&old_magic)?;
    assert_eq!(old_encoded, old_magic);
    let (decoded, _) = fixtures::decoded(old_encoded)?;
    assert_eq!(fixtures::binary(&decoded)?, old_magic);
    let magic_prefixed = b"FSV2\x02not-a-term";
    let encoded_magic = fixtures::encoded_binary(magic_prefixed)?;
    assert_ne!(encoded_magic, magic_prefixed);
    let (decoded, _) = fixtures::decoded(encoded_magic)?;
    assert_eq!(fixtures::binary(&decoded)?, magic_prefixed);
    let map = fixtures::encoded_map(b"kind", b"typed", false)?;
    let (decoded, _) = fixtures::decoded(map)?;
    assert_eq!(
        fixtures::map_fields(&decoded)?,
        (b"kind".as_slice(), b"typed".as_slice())
    );
    Ok(())
}

#[test]
fn source_typed_value_codec_returns_noncanonical_terms_without_deserializing() -> TestResult {
    let payload = b"typed".repeat(1_024);
    let encoded = fixtures::encoded_map(b"kind", &payload, false)?;
    assert_eq!(encoded.get(..5), Some(b"FSV2\x02".as_slice()));
    let external = encoded.get(5..).ok_or("typed suffix")?;
    assert_eq!(external, fixtures::expected_payload_term());
    let mut trailing = encoded.clone();
    trailing.push(0);
    let mut suffix = external.to_vec();
    suffix.push(0);
    let (decoded, _) = fixtures::decoded(trailing.clone())?;
    assert_eq!(fixtures::binary(&decoded)?, suffix);
    let (decoded, user_size) = fixtures::decoded(trailing)?;
    assert_eq!(
        (fixtures::binary(&decoded)?, user_size),
        (suffix.as_slice(), external.len() + 1)
    );
    let mut compressed = b"FSV2\x02".to_vec();
    compressed.extend_from_slice(fixtures::COMPRESSED);
    let (decoded, _) = fixtures::decoded(compressed)?;
    assert!(!matches!(decoded.root(), Some(N::Map { .. })));
    assert_eq!(fixtures::binary(&decoded)?, fixtures::COMPRESSED);
    assert!(
        matches!(super::decode(encoded, &fixtures::context(), fixtures::limits()), Ok(document) if matches!(document.root(), Some(N::Map { .. })))
    );
    Ok(())
}
