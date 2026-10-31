use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::governance::catalog::{
    CreditCatalogAckRejection as R, CreditCatalogAckReply as Reply,
    CreditCatalogAckRequest as Request, CreditCatalogMeta as Meta,
};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    request::{ClientIntent, RequestScope, RetainedStatus},
};
use wf_ports::log::{
    CodecError, CodecLimits, catalog_ack_retained_summary, decode_catalog_ack_reply,
    encode_catalog_ack_reply, encode_catalog_ack_request, prepare_catalog_ack_request,
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity},
};

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn stamp(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn independent() -> Vec<u8> {
    let mut body = 1_u64.to_be_bytes().to_vec();
    body.extend_from_slice(&2_u64.to_be_bytes());
    let mut hash = Sha256::new();
    hash.update(b"wf-credit-catalog-ack-intent-v1\0");
    hash.update(&body);
    let mut bytes = b"FWCP\0\x01\x0b".to_vec();
    for byte in *b"tgrq" {
        bytes.extend_from_slice(&[0, 0, 0, 1, byte]);
    }
    bytes.extend_from_slice(&13_u16.to_be_bytes());
    bytes.extend_from_slice(&hash.finalize());
    bytes.extend_from_slice(&1000_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&1000_u64.to_be_bytes());
    bytes.extend_from_slice(&body);
    bytes
}

#[test]
fn catalog_ack_request_matches_independent_bytes_digest_and_generated_time_exclusion() -> TestResult
{
    let scope = scope()?;
    let id = RequestId::new(b"q".to_vec())?;
    let command = Request {
        expected_head: 1,
        up_to: 2,
    };
    let frame = encode_catalog_ack_request(
        &scope,
        &id,
        stamp(1000)?,
        command,
        CodecLimits::default(),
        8192,
    )?;
    assert_eq!(frame.as_bytes(), independent());
    assert_eq!(frame.as_bytes().len(), 95);
    let decoded = prepare_catalog_ack_request(frame.as_bytes(), CodecLimits::default(), 8192)?;
    assert_eq!(decoded.command(), command);
    assert_eq!(decoded.as_bytes(), frame.as_bytes());
    assert_eq!(decoded.owned_bytes(), 4476);
    let log = LogIdentity::new(1, 2, AppliedPosition::new(7));
    let header = decoded.header(log);
    assert_eq!(
        (header.scope, header.request, header.intent.version()),
        (&scope, &id, 13)
    );
    assert_eq!(header.identity.log(), log);
    assert_eq!(
        header.identity.prepared().as_bytes(),
        Sha256::digest(frame.as_bytes()).as_slice()
    );
    let newer = encode_catalog_ack_request(
        &scope,
        &id,
        stamp(1001)?,
        command,
        CodecLimits::default(),
        8192,
    )?;
    let newer = prepare_catalog_ack_request(newer.as_bytes(), CodecLimits::default(), 8192)?;
    assert_eq!(header.intent, newer.header(log).intent);
    assert_ne!(
        header.identity.prepared(),
        newer.header(log).identity.prepared()
    );
    assert!(prepare_catalog_ack_request(frame.as_bytes(), CodecLimits::default(), 4475).is_err());
    assert!(prepare_catalog_ack_request(frame.as_bytes(), CodecLimits::default(), 4476).is_ok());
    Ok(())
}

#[test]
fn catalog_ack_request_rejects_every_truncation_tampering_and_invalid_controls_before_ownership()
-> TestResult {
    let bytes = independent();
    assert!(prepare_catalog_ack_request(&bytes, CodecLimits::default(), 8192).is_ok());
    for end in 0..bytes.len() {
        assert!(prepare_catalog_ack_request(&bytes[..end], CodecLimits::default(), 8192).is_err());
    }
    let mut wrong = bytes.clone();
    wrong[29] ^= 1;
    assert!(prepare_catalog_ack_request(&wrong, CodecLimits::default(), 8192).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(prepare_catalog_ack_request(&trailing, CodecLimits::default(), 8192).is_err());
    assert!(wf_ports::log::prepare_credit_request(&bytes, CodecLimits::default(), 8192).is_err());
    for request in [
        Request {
            expected_head: 0,
            up_to: 1,
        },
        Request {
            expected_head: 2,
            up_to: 1,
        },
        Request {
            expected_head: 1,
            up_to: 257,
        },
    ] {
        assert!(
            encode_catalog_ack_request(
                &scope()?,
                &RequestId::new(b"q".to_vec())?,
                stamp(1000)?,
                request,
                CodecLimits::default(),
                8192
            )
            .is_err()
        );
    }
    assert_eq!(
        prepare_catalog_ack_request(&bytes, CodecLimits::default(), 1).err(),
        Some(CodecError::OwnedLimit)
    );
    Ok(())
}

#[test]
fn catalog_ack_reply_has_fixed_complete_bytes_business_status_and_closed_old_profiles() -> TestResult
{
    let reply = Reply::Accepted {
        meta: Meta::restore(3, 5)?,
        deleted: 2,
    };
    let mut literal = b"FWCA\0\x01\0".to_vec();
    literal.extend_from_slice(&3_u64.to_be_bytes());
    literal.extend_from_slice(&5_u64.to_be_bytes());
    literal.extend_from_slice(&2_u16.to_be_bytes());
    assert_eq!(literal.len(), 25);
    assert_eq!(encode_catalog_ack_reply(reply, 25, 1024)?, literal);
    assert_eq!(decode_catalog_ack_reply(&literal, 25, 1024)?, reply);
    wf_ports::log::verify_catalog_ack_reply(reply, &literal)?;
    let intent = ClientIntent::credit_catalog_ack([0; 32]);
    assert_eq!(
        catalog_ack_retained_summary(intent, 11, &literal, AdmissionLimits::default())?,
        (RetainedStatus::Accepted, None)
    );
    for reason in [R::StaleCursor, R::InvalidRange] {
        let reply = Reply::Rejected {
            meta: Meta::restore(2, 5)?,
            reason,
        };
        let encoded = encode_catalog_ack_reply(reply, 25, 1024)?;
        assert_eq!(decode_catalog_ack_reply(&encoded, 25, 1024)?, reply);
        assert_eq!(
            catalog_ack_retained_summary(intent, 11, &encoded, AdmissionLimits::default())?,
            (RetainedStatus::CreditCatalogRejected(reason), None)
        );
        let mut wrong = encoded;
        wrong[24] = 1;
        assert!(decode_catalog_ack_reply(&wrong, 25, 1024).is_err());
    }
    for end in 0..literal.len() {
        assert!(decode_catalog_ack_reply(&literal[..end], 25, 1024).is_err());
    }
    let mut trailing = literal.clone();
    trailing.push(0);
    assert!(decode_catalog_ack_reply(&trailing, 26, 1024).is_err());
    assert!(decode_catalog_ack_reply(&literal, 24, 1024).is_err());
    assert!(decode_catalog_ack_reply(&literal, 25, 1023).is_err());
    assert!(
        catalog_ack_retained_summary(
            ClientIntent::governance_credit([0; 32]),
            11,
            &literal,
            AdmissionLimits::default()
        )
        .is_err()
    );
    assert!(
        catalog_ack_retained_summary(intent, 10, &literal, AdmissionLimits::default()).is_err()
    );
    assert!(
        wf_ports::log::credit_retained_summary(intent, 11, &literal, AdmissionLimits::default())
            .is_err()
    );
    Ok(())
}
