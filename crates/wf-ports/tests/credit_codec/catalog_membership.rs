use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::catalog::{CatalogMembershipAction as A, CatalogMembershipChange as C},
    workflow::request::{ClientIntent, RequestScope, RetainedStatus},
};
use wf_ports::log::{
    CodecLimits, catalog_membership_recovery_header, catalog_membership_request_intent,
    catalog_membership_retained_summary, encode_catalog_membership_request,
    prepare_catalog_membership_request,
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
    let mut body = vec![0];
    body.extend_from_slice(&1_u32.to_be_bytes());
    body.extend_from_slice(b"c");
    body.extend_from_slice(&3_u16.to_be_bytes());
    for member in [b"b".as_slice(), b"a\0", b"b"] {
        body.extend_from_slice(&u32::try_from(member.len()).unwrap_or(0).to_be_bytes());
        body.extend_from_slice(member);
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-governance-catalog-intent-v1\0");
    hash.update(&body);
    let mut frame = b"FWCP\0\x01\x0c".to_vec();
    for field in [b"t", b"g", b"r", b"i"] {
        frame.extend_from_slice(&1_u32.to_be_bytes());
        frame.extend_from_slice(field);
    }
    frame.extend_from_slice(&14_u16.to_be_bytes());
    frame.extend_from_slice(&hash.finalize());
    frame.extend_from_slice(&1000_u64.to_be_bytes());
    frame.extend_from_slice(&0_u16.to_be_bytes());
    frame.extend_from_slice(&1000_u64.to_be_bytes());
    frame.extend_from_slice(&body);
    frame
}

#[test]
fn catalog_membership_codec_matches_independent_vector_and_borrows_original_members() -> TestResult
{
    let frame = vector();
    let limits = CodecLimits::default();
    let prepared = prepare_catalog_membership_request(&frame, limits, 65536)?;
    assert_eq!(prepared.command().catalog, b"c");
    assert_eq!(prepared.command().members, &[b"b".as_slice(), b"a\0", b"b"]);
    for member in prepared.command().members {
        assert!(member.as_ptr() >= frame.as_ptr());
        assert!(member.as_ptr() < frame.as_ptr().wrapping_add(frame.len()));
    }
    let command = C {
        catalog: b"c",
        action: A::Register,
        members: &[b"b", b"a\0", b"b"],
    };
    let encoded = encode_catalog_membership_request(
        &scope()?,
        &RequestId::new(b"i".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(1000, 0)?, 1000)?,
        command,
        limits,
        65536,
    )?;
    assert_eq!(encoded.as_bytes(), frame);
    assert_eq!(
        prepared
            .header(wf_types::version::LogIdentity::new(
                1,
                1,
                wf_types::version::AppliedPosition::new(0)
            ))
            .intent,
        catalog_membership_request_intent(command)?
    );
    assert_eq!(catalog_membership_recovery_header(&frame, limits)?.0, 14);
    assert!(wf_ports::log::normal_recovery_header(&frame, limits).is_err());
    assert!(ClientIntent::new(14, [0; 32]).is_err());
    Ok(())
}

#[test]
fn catalog_membership_decoder_checks_every_truncation_hash_eof_and_owner_boundary() -> TestResult {
    let frame = vector();
    let limits = CodecLimits::default();
    let prepared = prepare_catalog_membership_request(&frame, limits, 65536)?;
    let owned = prepared.owned_bytes();
    assert!(prepare_catalog_membership_request(&frame, limits, owned).is_ok());
    assert!(prepare_catalog_membership_request(&frame, limits, owned - 1).is_err());
    for cut in 0..frame.len() {
        assert!(
            prepare_catalog_membership_request(&frame[..cut], limits, 65536).is_err(),
            "cut{cut}"
        );
    }
    let mut bad = frame.clone();
    bad.push(0);
    assert!(prepare_catalog_membership_request(&bad, limits, 65536).is_err());
    let last = frame.len() - 1;
    bad = frame;
    bad[last] ^= 1;
    assert!(prepare_catalog_membership_request(&bad, limits, 65536).is_err());
    let intent = ClientIntent::governance_catalog([0; 32]);
    assert_eq!(
        catalog_membership_retained_summary(intent, 12, b"FWGR\0\x01", limits.admission())?,
        (RetainedStatus::Accepted, None)
    );
    assert!(
        catalog_membership_retained_summary(intent, 11, b"FWGR\0\x01", limits.admission()).is_err()
    );
    assert!(
        catalog_membership_retained_summary(
            ClientIntent::credit_catalog_ack([0; 32]),
            12,
            b"FWGR\0\x01",
            limits.admission()
        )
        .is_err()
    );
    assert!(
        catalog_membership_retained_summary(intent, 12, b"FWGR\0\x01\0", limits.admission())
            .is_err()
    );
    Ok(())
}
