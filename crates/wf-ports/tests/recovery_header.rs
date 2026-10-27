//! Independent header grammar; successful dispatch does not verify a body.

#![forbid(unsafe_code)]

use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::{CodecError, CodecLimits, normal_recovery_header};
use wf_types::version::{CommittedTime, HlcTimestamp};

fn packed(root: u8, intent: u16, scopes: [&[u8]; 3]) -> Result<Vec<u8>, std::num::TryFromIntError> {
    let mut bytes = b"FWCP\0\x01".to_vec();
    bytes.push(root);
    for field in scopes.into_iter().chain([b"request".as_slice()]) {
        bytes.extend_from_slice(&u32::try_from(field.len())?.to_be_bytes());
        bytes.extend_from_slice(field);
    }
    bytes.extend_from_slice(&intent.to_be_bytes());
    bytes.extend_from_slice(&[0x5a; 32]);
    bytes.extend_from_slice(&17_u64.to_be_bytes());
    bytes.extend_from_slice(&3_u16.to_be_bytes());
    bytes.extend_from_slice(&11_u64.to_be_bytes());
    Ok(bytes)
}

#[test]
fn normal_recovery_header_borrows_supported_profiles_without_body_authority()
-> Result<(), Box<dyn std::error::Error>> {
    for (root, intent, profile) in [(1, 5, 5), (1, 6, 6), (4, 1, 1), (3, 1, 3)] {
        let scopes: [&[u8]; 3] = [b"t\0\xff", b"g\0", b"z\xff"];
        let mut bytes = packed(root, intent, scopes)?;
        for suffix in [&[][..], b"not a valid operation body".as_slice()] {
            let header_len = bytes.len();
            bytes.extend_from_slice(suffix);
            let (actual, borrowed, stamp) = normal_recovery_header(&bytes, CodecLimits::default())?;
            assert_eq!(actual, profile);
            assert_eq!(borrowed, scopes);
            assert_eq!(borrowed[0].as_ptr(), bytes[11..].as_ptr());
            assert_eq!(stamp, CommittedTime::new(HlcTimestamp::new(17, 3)?, 11)?);
            bytes.truncate(header_len);
        }
    }
    Ok(())
}

#[test]
fn normal_recovery_header_rejects_wrong_profiles_framing_and_byte_caps()
-> Result<(), Box<dyn std::error::Error>> {
    for (root, intent) in [(1, 1), (2, 1)] {
        assert_eq!(
            normal_recovery_header(
                &packed(root, intent, [b"t", b"g", b"z"])?,
                CodecLimits::default()
            ),
            Err(CodecError::UnsupportedTag)
        );
    }
    for intent in [5, 6] {
        assert_eq!(
            normal_recovery_header(
                &packed(3, intent, [b"t", b"g", b"z"])?,
                CodecLimits::default()
            ),
            Err(CodecError::UnsupportedVersion)
        );
    }
    let bytes = packed(1, 5, [b"t", b"g", b"z"])?;
    for length in 0..bytes.len() {
        assert!(
            normal_recovery_header(&bytes[..length], CodecLimits::default()).is_err(),
            "prefix {length}"
        );
    }
    let cap = CodecLimits::new(AdmissionLimits::new(1, bytes.len() - 1, 1)?);
    assert_eq!(
        normal_recovery_header(&bytes, cap),
        Err(CodecError::ByteLimit)
    );
    for (offset, byte, expected) in [
        (0, b'X', CodecError::Malformed),
        (5, 2, CodecError::UnsupportedVersion),
        (6, 9, CodecError::UnsupportedTag),
    ] {
        let mut changed = bytes.clone();
        changed[offset] = byte;
        assert_eq!(
            normal_recovery_header(&changed, CodecLimits::default()),
            Err(expected)
        );
    }
    for scopes in [
        [&[][..], b"g", b"z"],
        [&[0xff; 8193][..], b"g", b"z"],
        [b"t", &[1; 257][..], b"z"],
        [b"t", b"g", &[2; 257][..]],
    ] {
        assert!(normal_recovery_header(&packed(1, 5, scopes)?, CodecLimits::default()).is_err());
    }
    Ok(())
}

#[test]
fn normal_recovery_header_borrows_atomic_start_without_body_authority()
-> Result<(), Box<dyn std::error::Error>> {
    let scopes: [&[u8]; 3] = [b"t\0\xff", b"g\0", b"z\xff"];
    let header = packed(1, 7, scopes)?;
    for suffix in [&[][..], b"unverified atomic operation".as_slice()] {
        let mut bytes = header.clone();
        bytes.extend_from_slice(suffix);
        let cap = CodecLimits::new(AdmissionLimits::new(1, bytes.len(), 1)?);
        let (profile, borrowed, stamp) = normal_recovery_header(&bytes, cap)?;
        assert_eq!(profile, 7);
        assert_eq!(borrowed, scopes);
        let mut at = 7;
        for field in borrowed {
            at += 4;
            assert_eq!(field.as_ptr(), bytes[at..].as_ptr());
            at += field.len();
        }
        assert_eq!(stamp, CommittedTime::new(HlcTimestamp::new(17, 3)?, 11)?);
        let below = CodecLimits::new(AdmissionLimits::new(1, bytes.len() - 1, 1)?);
        assert_eq!(
            normal_recovery_header(&bytes, below),
            Err(CodecError::ByteLimit)
        );
    }
    assert_atomic_header_bounds(&header)?;
    let value_header = packed(1, 8, scopes)?;
    let (profile, borrowed, stamp) = normal_recovery_header(&value_header, CodecLimits::default())?;
    assert_eq!(profile, 8);
    assert_eq!(borrowed, scopes);
    assert_eq!(borrowed[0].as_ptr(), value_header[11..].as_ptr());
    assert_eq!(stamp, CommittedTime::new(HlcTimestamp::new(17, 3)?, 11)?);
    assert_eq!(
        normal_recovery_header(
            &packed(1, u16::MAX, [b"t", b"g", b"z"])?,
            CodecLimits::default()
        ),
        Err(CodecError::UnsupportedVersion)
    );
    for root in [1, 2] {
        let legacy = packed(root, 1, [b"t", b"g", b"z"])?;
        assert_eq!(
            normal_recovery_header(&legacy, CodecLimits::default()),
            Err(CodecError::UnsupportedTag)
        );
        let mut truncated = legacy[..legacy.len() - 1].to_vec();
        assert_eq!(
            normal_recovery_header(&truncated, CodecLimits::default()),
            Err(CodecError::Malformed)
        );
        truncated = legacy;
        let start = truncated.len() - 18;
        truncated[start..start + 8].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(
            normal_recovery_header(&truncated, CodecLimits::default()),
            Err(CodecError::Malformed)
        );
    }
    Ok(())
}

fn assert_atomic_header_bounds(header: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    for length in 0..header.len() {
        assert!(normal_recovery_header(&header[..length], CodecLimits::default()).is_err());
    }
    for (offset, byte, expected) in [
        (0, b'X', CodecError::Malformed),
        (5, 2, CodecError::UnsupportedVersion),
        (6, 2, CodecError::UnsupportedVersion),
        (6, 6, CodecError::UnsupportedTag),
    ] {
        let mut changed = header.to_vec();
        changed[offset] = byte;
        assert_eq!(
            normal_recovery_header(&changed, CodecLimits::default()),
            Err(expected)
        );
    }
    for scopes in [
        [&[][..], b"g", b"z"],
        [&[0xff; 8193][..], b"g", b"z"],
        [b"t", &[1; 257][..], b"z"],
        [b"t", b"g", &[2; 257][..]],
    ] {
        assert!(normal_recovery_header(&packed(1, 7, scopes)?, CodecLimits::default()).is_err());
    }
    for scopes in [[b"t".as_slice(), &[], b"z"], [b"t".as_slice(), b"g", &[]]] {
        assert!(normal_recovery_header(&packed(1, 7, scopes)?, CodecLimits::default()).is_err());
    }
    let exact = packed(1, 7, [&[1; 8192], &[2; 256], &[3; 256]])?;
    assert_eq!(normal_recovery_header(&exact, CodecLimits::default())?.0, 7);
    for request_len in [0_u32, 256, 257] {
        let mut changed = packed(1, 7, [b"t", b"g", b"z"])?;
        changed[22..26].copy_from_slice(&request_len.to_be_bytes());
        drop(changed.splice(26..33, vec![b'q'; usize::try_from(request_len)?]));
        assert_eq!(
            normal_recovery_header(&changed, CodecLimits::default()).is_ok(),
            request_len == 256
        );
    }
    Ok(())
}
