//! Alias wire versions distinguish representation without decoding content.

use sha2::{Digest, Sha256};

use super::{Error, OriginalValueRefKind, TimestampMillis, ValueAlias, decode_alias, encode_alias};

fn alias(stored: bool) -> ValueAlias<'static> {
    ValueAlias {
        partition: b"p",
        id: b"i",
        kind: OriginalValueRefKind::Payload,
        version: 3,
        incarnation: 42,
        expires_at: Some(TimestampMillis::new(250)),
        stored_len: 27,
        digest: [7; 32],
        stored,
    }
}

fn reseal(bytes: &mut [u8]) -> Result<(), Error> {
    let end = bytes.len().checked_sub(32).ok_or(Error::Malformed)?;
    let checksum = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&checksum);
    Ok(())
}

#[test]
fn stored_alias_codec_emits_explicit_version_and_keeps_old_bytes() -> Result<(), Error> {
    let binary = encode_alias(alias(false), 1_024)?;
    assert_eq!(&binary[..6], b"FWVA\0\x01");
    let stored = encode_alias(alias(true), 1_024);
    assert!(stored.is_ok(), "{:?}", stored.as_ref().err());
    let stored = stored?;
    assert_eq!(&stored[..6], b"FWVA\0\x02");
    assert_eq!(stored.len(), 121);
    assert_eq!(binary.len(), stored.len());
    assert_eq!(&stored[6..89], &binary[6..89]);
    assert_ne!(&stored[89..], &binary[89..]);
    let decoded = decode_alias(&stored)?;
    assert!(decoded.stored);
    assert_eq!(decoded.partition, b"p");
    assert_eq!(decoded.id, b"i");
    assert_eq!(decoded.kind, OriginalValueRefKind::Payload);
    assert_eq!(decoded.version, 3);
    assert_eq!(decoded.incarnation, 42);
    assert_eq!(decoded.expires_at, Some(TimestampMillis::new(250)));
    assert_eq!(decoded.stored_len, 27);
    assert_eq!(decoded.digest, [7; 32]);
    assert!(!decode_alias(&binary)?.stored);
    let mut normalized = stored;
    normalized[4..6].copy_from_slice(&1_u16.to_be_bytes());
    reseal(&mut normalized)?;
    assert_eq!(normalized, binary);
    Ok(())
}

#[test]
fn stored_alias_codec_rejects_corruption_without_decoding_content() -> Result<(), Error> {
    let mut data = alias(true);
    let malformed_typed = b"FSV2\x02not-an-external-term";
    data.stored_len = u64::try_from(malformed_typed.len()).map_err(|_| Error::LengthOverflow)?;
    data.digest = Sha256::digest(malformed_typed).into();
    let encoded = encode_alias(data, 1_024);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    assert_eq!(&encoded[..6], b"FWVA\0\x02");
    let decoded = decode_alias(&encoded)?;
    assert!(decoded.stored);
    assert_eq!(decoded.stored_len, malformed_typed.len() as u64);
    assert_eq!(
        decoded.digest,
        <[u8; 32]>::from(Sha256::digest(malformed_typed))
    );
    for end in 0..encoded.len() {
        assert!(decode_alias(&encoded[..end]).is_err(), "prefix {end}");
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(decode_alias(&trailing).is_err());
    let mut bad_checksum = encoded.clone();
    bad_checksum[20] ^= 1;
    assert!(decode_alias(&bad_checksum).is_err());
    let mut unknown = encoded;
    unknown[4..6].copy_from_slice(&3_u16.to_be_bytes());
    reseal(&mut unknown)?;
    assert!(matches!(
        decode_alias(&unknown),
        Err(Error::UnsupportedVersion)
    ));
    Ok(())
}
