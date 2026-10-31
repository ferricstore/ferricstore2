//! Independent progress frames and recoverable hint bytes, with complete bounds.
use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::catalog::CatalogProgressChange as C,
    workflow::request::{RequestScope, RetainedStatus},
};
use wf_ports::{
    log::{
        CodecError as E, CodecLimits, catalog_progress_recovery_header,
        catalog_progress_request_intent, catalog_progress_retained_summary,
        encode_catalog_progress_request, prepare_catalog_progress_request,
    },
    state::{encode_catalog_progress_cursor, inspect_catalog_progress_cursor},
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};
fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn vector() -> Vec<u8> {
    let mut body = vec![0, 0, 0, 1, b'c', 1, 0, 0, 0, 3, b'a', 0, b'z'];
    let mut hash = Sha256::new();
    hash.update(b"wf-governance-catalog-progress-intent-v1\0");
    hash.update(&body);
    let mut frame = b"FWCP\0\x01\x0d".to_vec();
    for value in [b't', b'g', b'r', b'i'] {
        frame.extend_from_slice(&1_u32.to_be_bytes());
        frame.push(value);
    }
    frame.extend_from_slice(&15_u16.to_be_bytes());
    frame.extend_from_slice(&hash.finalize());
    frame.extend_from_slice(&1000_u64.to_be_bytes());
    frame.extend_from_slice(&0_u16.to_be_bytes());
    frame.extend_from_slice(&1000_u64.to_be_bytes());
    frame.append(&mut body);
    frame
}
#[test]
fn catalog_progress_domain_preserves_clear_empty_and_bounded_binary_fields() -> TestResult {
    let clear = C {
        catalog: b"c",
        cursor: None,
    };
    let empty = C {
        catalog: b"c",
        cursor: Some(b""),
    };
    clear.validate(65535)?;
    empty.validate(65535)?;
    assert_ne!(
        catalog_progress_request_intent(clear)?,
        catalog_progress_request_intent(empty)?
    );
    assert!(clear.validate(0).is_err());
    let over = vec![0; 65_536];
    assert!(
        C {
            catalog: b"c",
            cursor: Some(&over)
        }
        .validate(65535)
        .is_err()
    );
    Ok(())
}
#[test]
fn catalog_progress_codec_matches_independent_frame_and_borrows_original_cursor() -> TestResult {
    let frame = vector();
    let limits = CodecLimits::default();
    let prepared = prepare_catalog_progress_request(&frame, limits, 65536)?;
    let command = C {
        catalog: b"c",
        cursor: Some(b"a\0z"),
    };
    assert_eq!(prepared.command(), command);
    assert_eq!(
        prepared.command().cursor.map(<[u8]>::as_ptr),
        Some(frame[frame.len() - 3..].as_ptr())
    );
    let stamp = CommittedTime::new(HlcTimestamp::new(1000, 0)?, 1000)?;
    let encoded = encode_catalog_progress_request(
        &scope()?,
        &RequestId::new(b"i".to_vec())?,
        stamp,
        command,
        limits,
        65536,
    )?;
    assert_eq!(encoded.as_bytes(), frame);
    assert_eq!(
        catalog_progress_recovery_header(&frame, limits)?,
        (15, [b"t".as_slice(), b"g", b"r"], stamp)
    );
    assert_eq!(
        catalog_progress_retained_summary(
            catalog_progress_request_intent(command)?,
            13,
            b"FWPR\0\x01",
            limits.admission()
        )?,
        (RetainedStatus::Accepted, None)
    );
    for end in 0..frame.len() {
        assert!(prepare_catalog_progress_request(&frame[..end], limits, 65536).is_err());
    }
    let mut bad = frame.clone();
    bad.push(0);
    assert!(prepare_catalog_progress_request(&bad, limits, 65536).is_err());
    let mut changed = frame.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert_eq!(
        prepare_catalog_progress_request(&changed, limits, 65536).err(),
        Some(E::Malformed)
    );
    let owned = prepared.owned_bytes();
    assert!(prepare_catalog_progress_request(&frame, limits, owned).is_ok());
    assert_eq!(
        prepare_catalog_progress_request(&frame, limits, owned - 1).err(),
        Some(E::OwnedLimit)
    );
    Ok(())
}
#[test]
fn catalog_progress_hint_codec_is_complete_borrowed_and_classifies_invalid_without_reset()
-> TestResult {
    let bytes = encode_catalog_progress_cursor(b"a\0z", 13, 141)?;
    assert_eq!(bytes, b"FWPC\0\x01\0\0\0\x03a\0z");
    assert_eq!(
        inspect_catalog_progress_cursor(&bytes),
        Some(b"a\0z".as_slice())
    );
    assert_eq!(
        inspect_catalog_progress_cursor(&bytes).map(<[u8]>::as_ptr),
        Some(bytes[10..].as_ptr())
    );
    for end in 0..bytes.len() {
        assert_eq!(inspect_catalog_progress_cursor(&bytes[..end]), None);
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(inspect_catalog_progress_cursor(&trailing), None);
    let empty = encode_catalog_progress_cursor(b"", 10, 138)?;
    assert_eq!(
        inspect_catalog_progress_cursor(&empty),
        Some(b"".as_slice())
    );
    assert_eq!(
        encode_catalog_progress_cursor(b"a\0z", 12, 141),
        Err(E::ByteLimit)
    );
    assert_eq!(
        encode_catalog_progress_cursor(b"a\0z", 13, 140),
        Err(E::OwnedLimit)
    );
    Ok(())
}
