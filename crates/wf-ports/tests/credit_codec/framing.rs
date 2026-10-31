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

pub(super) fn independent_owner(leased: bool) -> Vec<u8> {
    let mut bytes = b"FWGO\0\x01\0\0\0\x01x".to_vec();
    for n in if leased {
        [3_u64, 0, 1, 1]
    } else {
        [2, 2, 0, 1]
    } {
        bytes.extend_from_slice(&n.to_be_bytes());
    }
    bytes.push(0);
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u64.to_be_bytes());
    bytes.extend_from_slice(&u32::from(leased).to_be_bytes());
    if leased {
        for n in [0_u64, 1, 2000, 1, 2, 0, 0] {
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        bytes.push(1);
        bytes.extend_from_slice(&1001_u64.to_be_bytes());
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&2_u16.to_be_bytes());
    }
    bytes
}

#[test]
fn credit_owner_independent_empty_literal_preserves_capacity_and_refuses_tampering() -> TestResult {
    let bytes = independent_owner(false);
    assert_eq!(bytes.len(), 64);
    let owner = wf_ports::log::decode_credit_owner(&bytes, CreditLimits::default(), 1024, 4096)?;
    assert_eq!(
        (
            owner.view().scope.as_slice(),
            owner.view().limit,
            owner.view().free
        ),
        (b"x".as_slice(), 2, 2)
    );
    assert_eq!((owner.view().epoch, owner.view().config_version), (0, 1));
    assert_eq!(
        wf_ports::log::encode_credit_owner(&owner, 1024, 4096)?,
        bytes
    );
    for end in 0..bytes.len() {
        assert!(
            wf_ports::log::decode_credit_owner(&bytes[..end], CreditLimits::default(), 1024, 4096)
                .is_err()
        );
    }
    let mut wrong = bytes.clone();
    wrong[19..27].copy_from_slice(&1_u64.to_be_bytes());
    assert!(
        wf_ports::log::decode_credit_owner(&wrong, CreditLimits::default(), 1024, 4096).is_err()
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(
        wf_ports::log::decode_credit_owner(&trailing, CreditLimits::default(), 1024, 4096).is_err()
    );
    assert!(wf_ports::log::decode_credit_owner(&bytes, CreditLimits::default(), 63, 4096).is_err());
    assert!(wf_ports::log::decode_credit_owner(&bytes, CreditLimits::default(), 1024, 1).is_err());
    Ok(())
}

#[test]
fn credit_reply_independent_detached_lease_preserves_original_binary_id_order() -> TestResult {
    let owner = independent_owner(true);
    assert_eq!(owner.len(), 133);
    let mut bytes = b"FWGR\0\x01\x02".to_vec();
    bytes.extend_from_slice(&0_u64.to_be_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&133_u32.to_be_bytes());
    bytes.extend_from_slice(&owner);
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    for id in [b"\0a", b"\xffb"] {
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend_from_slice(id);
    }
    assert_eq!(bytes.len(), 169);
    let reply = decode_credit_reply(&bytes, CreditLimits::default(), 1024, 16_384)?;
    assert_eq!(
        reply.decision,
        IdentifiedCreditDecision::Spent { shard_id: 0 }
    );
    assert_eq!(
        reply.reservation_ids,
        vec![b"\0a".to_vec(), b"\xffb".to_vec()]
    );
    let lease = reply
        .owner
        .as_ref()
        .ok_or("missing owner")?
        .lease(0)
        .ok_or("missing lease")?;
    assert_eq!(
        (
            lease.epoch,
            lease.expires_at_ms,
            lease.available,
            lease.in_use
        ),
        (1, 2000, 1, 2)
    );
    assert_eq!(
        (
            lease.last_spend_at_ms,
            lease.reservation_page,
            lease.reservation_page_fill
        ),
        (Some(1001), 1, 2)
    );
    assert_eq!(lease.reservations.len(), 0);
    assert_eq!(
        wf_ports::log::encode_credit_reply(&reply, 1024, 16_384)?,
        bytes
    );
    for end in 0..bytes.len() {
        assert!(decode_credit_reply(&bytes[..end], CreditLimits::default(), 1024, 16_384).is_err());
    }
    for (at, value) in [(72, 0_u64), (104, 2), (112, f64::MAX.to_bits())] {
        let mut wrong = owner.clone();
        wrong[at..at + 8].copy_from_slice(&value.to_be_bytes());
        assert!(
            wf_ports::log::decode_credit_owner(&wrong, CreditLimits::default(), 1024, 16_384)
                .is_err()
        );
    }
    let mut duplicate = bytes;
    duplicate[167..169].copy_from_slice(b"\0a");
    assert!(decode_credit_reply(&duplicate, CreditLimits::default(), 1024, 16_384).is_err());
    Ok(())
}

fn check_prepared_case(mutation: IdentifiedCreditMutation<'_>) -> TestResult {
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    );
    let request = RequestId::new(b"r".to_vec())?;
    let stamp = CommittedTime::new(HlcTimestamp::new(1000, 0)?, 1000)?;
    let configuration = matches!(
        mutation,
        IdentifiedCreditMutation::Lease { .. } | IdentifiedCreditMutation::Spend { .. }
    )
    .then_some(CreditConfiguration {
        limit: 3,
        config_version: Some(1),
        policy_fingerprint: None,
    });
    let command = IdentifiedCreditRequest {
        scope: b"x",
        shard_count: 4,
        configuration,
        mutation,
    };
    let encoded = encode_credit_request(
        &scope,
        &request,
        stamp,
        command,
        CodecLimits::default(),
        1_048_576,
    )?;
    let decoded = wf_ports::log::prepare_credit_request(
        encoded.as_bytes(),
        CodecLimits::default(),
        1_048_576,
    )?;
    assert_eq!(decoded.command(), command);
    assert_eq!(decoded.as_bytes(), encoded.as_bytes());
    let log = wf_types::version::LogIdentity::new(1, 2, wf_types::version::AppliedPosition::new(7));
    let header = decoded.header(log);
    assert_eq!(header.scope, &scope);
    assert_eq!(header.request, &request);
    assert_eq!(header.stamp, stamp);
    assert_eq!(header.identity.log(), log);
    assert_eq!(header.identity.prepared(), encoded.digest());
    assert_eq!(header.intent.version(), 12);
    assert_eq!(header.encoded_entry, encoded.as_bytes());
    assert_eq!(
        header.encoded_isolation_scope,
        b"FWSC\0\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01z"
    );
    let newer = encode_credit_request(
        &scope,
        &request,
        CommittedTime::new(HlcTimestamp::new(1001, 0)?, 1001)?,
        command,
        CodecLimits::default(),
        1_048_576,
    )?;
    let newer =
        wf_ports::log::prepare_credit_request(newer.as_bytes(), CodecLimits::default(), 1_048_576)?;
    assert_eq!(newer.header(log).intent, header.intent);
    assert_ne!(
        newer.header(log).identity.prepared(),
        header.identity.prepared()
    );
    assert!(
        wf_ports::log::prepare_credit_request(
            encoded.as_bytes(),
            CodecLimits::default(),
            decoded.owned_bytes() - 1
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn credit_preparation_binds_all_mutations_original_ids_scope_and_clock() -> TestResult {
    let ids = vec![vec![0, 1], vec![255, 1]];
    for mutation in [
        IdentifiedCreditMutation::Lease {
            shard_id: 0,
            amount: 2,
            ttl_ms: 1000,
        },
        IdentifiedCreditMutation::Spend {
            shard_id: 0,
            lease_epoch: 1,
            amount: 2,
            ttl_ms: Some(1000),
            reservation_ids: &ids,
        },
        IdentifiedCreditMutation::Release {
            shard_id: 0,
            amount: 2,
            reservation_ids: &ids,
        },
        IdentifiedCreditMutation::Renew {
            shard_id: 0,
            ttl_ms: 1000,
        },
        IdentifiedCreditMutation::Get,
        IdentifiedCreditMutation::Cleanup,
    ] {
        check_prepared_case(mutation)?;
    }
    Ok(())
}

#[test]
fn credit_preparation_rejects_truncation_changed_digest_duplicates_and_tight_profiles() -> TestResult
{
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    );
    let ids = vec![vec![0, 1], vec![255, 1]];
    let frame = encode_credit_request(
        &scope,
        &RequestId::new(b"r".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(1000, 0)?, 1000)?,
        IdentifiedCreditRequest {
            scope: b"x",
            shard_count: 4,
            configuration: None,
            mutation: IdentifiedCreditMutation::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &ids,
            },
        },
        CodecLimits::default(),
        1_048_576,
    )?;
    assert!(
        wf_ports::log::prepare_credit_request(frame.as_bytes(), CodecLimits::default(), 1_048_576)
            .is_ok()
    );
    for end in 0..frame.as_bytes().len() {
        assert!(
            wf_ports::log::prepare_credit_request(
                &frame.as_bytes()[..end],
                CodecLimits::default(),
                1_048_576
            )
            .is_err()
        );
    }
    let mut wrong = frame.as_bytes().to_vec();
    wrong[29] ^= 1;
    assert!(
        wf_ports::log::prepare_credit_request(&wrong, CodecLimits::default(), 1_048_576).is_err()
    );
    let mut duplicate = frame.as_bytes().to_vec();
    let end = duplicate.len();
    duplicate[end - 2..].copy_from_slice(&[0, 1]);
    let mut hash = Sha256::new();
    hash.update(b"wf-governing-credit-intent-v1\0");
    hash.update(&duplicate[79..]);
    duplicate[29..61].copy_from_slice(&hash.finalize());
    assert!(
        wf_ports::log::prepare_credit_request(&duplicate, CodecLimits::default(), 1_048_576)
            .is_err()
    );
    let mut extra = frame.as_bytes().to_vec();
    extra.push(0);
    assert!(
        wf_ports::log::prepare_credit_request(&extra, CodecLimits::default(), 1_048_576).is_err()
    );
    assert!(
        wf_ports::log::prepare_credit_request(frame.as_bytes(), CodecLimits::default(), 1).is_err()
    );
    let defaults = CodecLimits::default().admission();
    let narrow = wf_domain::workflow::admission::AdmissionLimits::new(
        defaults.inline_bytes(),
        defaults.decision_bytes(),
        1,
    )?;
    assert!(
        wf_ports::log::prepare_credit_request(
            frame.as_bytes(),
            CodecLimits::new(narrow),
            1_048_576
        )
        .is_err()
    );
    Ok(())
}
