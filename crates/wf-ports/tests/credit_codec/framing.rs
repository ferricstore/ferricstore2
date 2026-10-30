use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::{
    CreditConfiguration, IdentifiedCreditDecision, IdentifiedCreditMutation,
    IdentifiedCreditRejection, IdentifiedCreditRequest,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{CodecLimits, decode_credit_reply, encode_credit_request};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

#[test]
fn credit_request_healthy_lease_matches_independent_full_bytes_and_digest() -> TestResult {
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    );
    let mut body = vec![0, 0, 0, 1, b's'];
    body.extend_from_slice(&4_u64.to_be_bytes());
    body.push(1);
    body.extend_from_slice(&2_u64.to_be_bytes());
    body.push(1);
    body.extend_from_slice(&1_u64.to_be_bytes());
    body.push(0);
    body.push(1);
    for n in [0_u64, 2, 1000] {
        body.extend_from_slice(&n.to_be_bytes());
    }
    let mut intent = Sha256::new();
    intent.update(b"wf-governing-credit-intent-v1\0");
    intent.update(&body);
    let mut expected = b"FWCP\0\x01\x0a".to_vec();
    for part in *b"tgzr" {
        expected.extend_from_slice(&[0, 0, 0, 1, part]);
    }
    expected.extend_from_slice(&12_u16.to_be_bytes());
    expected.extend_from_slice(&intent.finalize());
    expected.extend_from_slice(&1000_u64.to_be_bytes());
    expected.extend_from_slice(&0_u16.to_be_bytes());
    expected.extend_from_slice(&1000_u64.to_be_bytes());
    expected.extend_from_slice(&body);
    let actual = encode_credit_request(
        &scope,
        &RequestId::new(b"r".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(1000, 0)?, 1000)?,
        IdentifiedCreditRequest {
            scope: b"s",
            shard_count: 4,
            configuration: Some(CreditConfiguration {
                limit: 2,
                config_version: Some(1),
                policy_fingerprint: None,
            }),
            mutation: IdentifiedCreditMutation::Lease {
                shard_id: 0,
                amount: 2,
                ttl_ms: 1000,
            },
        },
        CodecLimits::default(),
        1_048_576,
    )?;
    assert_eq!(actual.as_bytes(), expected);
    assert_eq!(
        actual.digest().as_bytes(),
        Sha256::digest(&expected).as_slice()
    );
    Ok(())
}

#[test]
fn credit_reply_independent_not_found_vector_has_explicit_absence_and_eof() -> TestResult {
    let bytes = [b'F', b'W', b'G', b'R', 0, 1, 255, 1, 0, 0, 0, 0, 0];
    let reply = decode_credit_reply(&bytes, CreditLimits::default(), 1024, 4096)?;
    assert_eq!(
        reply.decision,
        IdentifiedCreditDecision::Rejected(IdentifiedCreditRejection::NotFound)
    );
    assert!(reply.owner.is_none());
    assert_eq!(reply.reservation_ids.len(), 0);
    for end in 0..bytes.len() {
        assert!(decode_credit_reply(&bytes[..end], CreditLimits::default(), 1024, 4096).is_err());
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode_credit_reply(&trailing, CreditLimits::default(), 1024, 4096).is_err());
    assert!(decode_credit_reply(&bytes, CreditLimits::default(), bytes.len() - 1, 4096).is_err());
    assert!(decode_credit_reply(&bytes, CreditLimits::default(), 1024, 1).is_err());
    Ok(())
}
