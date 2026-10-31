//! Independent public caller frames, ETF fingerprints and generated-ID recipes.
use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::{
        caller::{
            CreditCallKind as K, CreditOptionValue as V, CreditPolicyInput as P,
            normalize_credit_call,
        },
        identified::IdentifiedCreditMutation as Op,
    },
    workflow::request::RequestScope,
};
use wf_ports::log::{
    CodecError as E, CodecLimits, CreditSpendPreparation as Spend, encode_public_credit_request,
    fingerprint_credit_policy, generate_public_credit_ids, prepare_public_credit_request,
    public_credit_intent, public_credit_recovery_header,
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
fn clock(n: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(n, 0)?, n)?)
}
fn raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}
#[test]
fn public_credit_policy_fingerprint_matches_independent_external_term_recipes() -> TestResult {
    let vectors = [
        (0, vec![131, 97, 0]),
        (255, vec![131, 97, 255]),
        (256, vec![131, 98, 0, 0, 1, 0]),
        (2_147_483_648, vec![131, 110, 4, 0, 0, 0, 0, 128]),
        (
            9_007_199_254_740_991,
            vec![131, 110, 7, 0, 255, 255, 255, 255, 255, 255, 31],
        ),
    ];
    for (number, bytes) in vectors {
        assert_eq!(
            fingerprint_credit_policy(P::Number(number))?
                .as_bytes()
                .as_slice(),
            Sha256::digest(bytes).as_slice()
        );
    }
    let value = b"policy-a";
    let mut bytes = vec![131, 109];
    bytes.extend_from_slice(&8_u32.to_be_bytes());
    bytes.extend_from_slice(value);
    assert_eq!(
        fingerprint_credit_policy(P::Bytes(value))?
            .as_bytes()
            .as_slice(),
        Sha256::digest(bytes).as_slice()
    );
    Ok(())
}
#[test]
fn public_credit_ids_use_one_source_batch_and_preserve_exact_order() -> TestResult {
    let ids = generate_public_credit_ids(
        2,
        Spend {
            lease_epoch: 7,
            batch: [0; 16],
        },
        4096,
    )?;
    assert_eq!(
        ids,
        vec![
            b"flr1:7:AAAAAAAAAAAAAAAAAAAAAA:1".to_vec(),
            b"flr1:7:AAAAAAAAAAAAAAAAAAAAAA:2".to_vec()
        ]
    );
    assert_eq!(
        generate_public_credit_ids(
            2,
            Spend {
                lease_epoch: 7,
                batch: [0; 16]
            },
            1
        )
        .err(),
        Some(E::OwnedLimit)
    );
    Ok(())
}
#[test]
fn public_credit_get_frame_matches_complete_independent_header_intent_and_body() -> TestResult {
    let call = normalize_credit_call(K::Get, b"scope", 4, &[("now_ms", V::Integer(1000))], 65535)?;
    let frame = encode_public_credit_request(
        &scope()?,
        &RequestId::new(b"id".to_vec())?,
        call,
        clock(1000)?,
        None,
        CodecLimits::default(),
        65535,
    )?;
    let mut body = Vec::new();
    raw(&mut body, b"scope")?;
    body.extend_from_slice(&4_u64.to_be_bytes());
    body.extend_from_slice(&[5, 1]);
    body.extend_from_slice(&1000_u64.to_be_bytes());
    let mut hash = Sha256::new();
    hash.update(b"wf-public-credit-intent-v1\0");
    hash.update(&body);
    let intent = hash.finalize();
    let mut bytes = b"FWCP\0\x01\x0f".to_vec();
    for value in [b"t".as_slice(), b"g", b"r", b"id"] {
        raw(&mut bytes, value)?;
    }
    bytes.extend_from_slice(&17_u16.to_be_bytes());
    bytes.extend_from_slice(&intent);
    bytes.extend_from_slice(&1000_u64.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&1000_u64.to_be_bytes());
    bytes.extend_from_slice(&body);
    assert_eq!(frame.as_bytes(), bytes);
    assert_eq!(public_credit_intent(call)?.digest(), intent.as_slice());
    let prepared = prepare_public_credit_request(frame.as_bytes(), CodecLimits::default(), 65535)?;
    assert_eq!(prepared.command().scope, b"scope");
    assert_eq!(prepared.command().mutation, Op::Get);
    assert_eq!(
        public_credit_recovery_header(frame.as_bytes(), CodecLimits::default())?.0,
        17
    );
    Ok(())
}
#[test]
fn public_credit_spend_intent_excludes_trusted_time_epoch_and_batch_but_frame_binds_them()
-> TestResult {
    let opts = [("shard_id", V::Integer(0)), ("amount", V::Integer(2))];
    let call = normalize_credit_call(K::Spend, b"scope", 4, &opts, 65535)?;
    let namespace = scope()?;
    let id = RequestId::new(b"same".to_vec())?;
    let first = encode_public_credit_request(
        &namespace,
        &id,
        call,
        clock(1000)?,
        Some(Spend {
            lease_epoch: 7,
            batch: [0; 16],
        }),
        CodecLimits::default(),
        65535,
    )?;
    let second = encode_public_credit_request(
        &namespace,
        &id,
        call,
        clock(2000)?,
        Some(Spend {
            lease_epoch: 8,
            batch: [1; 16],
        }),
        CodecLimits::default(),
        65535,
    )?;
    assert_ne!(first.as_bytes(), second.as_bytes());
    assert_ne!(first.digest(), second.digest());
    let first = prepare_public_credit_request(first.as_bytes(), CodecLimits::default(), 65535)?;
    let second = prepare_public_credit_request(second.as_bytes(), CodecLimits::default(), 65535)?;
    let log = LogIdentity::new(1, 1, AppliedPosition::new(0));
    assert_eq!(first.header(log).intent, second.header(log).intent);
    assert_eq!(first.header(log).intent, public_credit_intent(call)?);
    let Op::Spend {
        lease_epoch,
        reservation_ids,
        ..
    } = first.command().mutation
    else {
        return Err("missing spend".into());
    };
    assert_eq!(lease_epoch, 7);
    assert_eq!(reservation_ids[0], b"flr1:7:AAAAAAAAAAAAAAAAAAAAAA:1");
    Ok(())
}
