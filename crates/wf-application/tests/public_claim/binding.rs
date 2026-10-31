//! Independent valid literal postimages; these tests grant no commit authority.
use super::{fixtures::*, *};
use wf_ports::log::{
    PublicClaimReplyLimits, encode_public_claim_reply,
    inspect_public_claim_reply_for_call as inspect,
};

fn raw(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn response() -> TestResult<Vec<u8>> {
    let mut core = b"FWCR\0\x01\0\0\0".to_vec();
    for n in [1_u32, 0, 1] {
        core.extend_from_slice(&n.to_be_bytes());
    }
    for bytes in [
        b"p".as_slice(),
        b"id",
        b"r",
        b"q",
        b"worker",
        b"worker:1000:1",
    ] {
        raw(&mut core, bytes)?;
    }
    for n in [2_u64, 1, 1100, 500, 1000, 0] {
        core.extend_from_slice(&n.to_be_bytes());
    }
    core.push(0);
    core.extend_from_slice(&1_u128.to_be_bytes());
    core.extend_from_slice(&[0, 0, 0]);
    let mut claim = b"FWAR\0\x01\0\0\0\0\x01\x01".to_vec();
    raw(&mut claim, &core)?;
    Ok(encode_public_claim_reply(
        &claim,
        TimestampMillis::new(1000),
        CodecLimits::default().admission(),
        1_048_576,
    )?)
}
fn profile() -> PublicClaimReplyLimits {
    PublicClaimCallLimits::default().reply
}
#[test]
fn full_caller_binding_accepts_the_original_literal_without_treating_requested_now_as_clock()
-> TestResult {
    let caller = caller()?;
    let body = response()?;
    let reply = inspect(&body, TimestampMillis::new(1000), &caller, profile())?;
    assert_eq!(reply.claim().ok_or("claim")?.count(), 1);
    Ok(())
}
#[test]
fn well_formed_foreign_worker_type_partition_state_priority_or_lease_is_not_an_original_reply()
-> TestResult {
    let body = response()?;
    for n in 0..6 {
        let mut input = ClaimDueInput::new(
            if n == 0 { b"other" } else { b"r" },
            if n == 1 { b"other" } else { b"worker" },
        );
        input.states = ClaimStatesInput::One(if n == 2 { b"other" } else { b"q" });
        input.partitions = ClaimPartitionsInput::One(if n == 3 { b"other" } else { b"p" });
        input.priority = (n == 4).then_some(2);
        input.lease_ms = Some(if n == 5 { 200 } else { 100 });
        input.limit = Some(2);
        input.reclaim_expired = Some(false);
        let caller = normalize_claim_due(input, ClaimCallerLimits::default())?;
        assert!(
            inspect(&body, TimestampMillis::new(1000), &caller, profile()).is_err(),
            "foreign caller field {n}"
        );
    }
    Ok(())
}

fn paid_response(scope: &[u8], ids: &[Vec<u8>]) -> TestResult<Vec<u8>> {
    let body = response()?;
    let old = body.get(11..).ok_or("literal original Claim span")?;
    let payment = wf_ports::log::encode_claim_payment(
        wf_ports::log::GovernedClaimPayment {
            scope,
            shard_id: 0,
            epoch: 1,
            enforcement: wf_domain::governance::release::LimitEnforcement::ApproximateGlobal,
            reservation_ids: ids,
        },
        CodecLimits::default(),
        1_048_576,
    )?;
    let mut claim = b"FWAR\0\x01\x01".to_vec();
    claim.extend_from_slice(&payment);
    claim.extend_from_slice(&[0, 0, 1, 0, 0]);
    claim.extend_from_slice(old.get(10..).ok_or("complete literal phases")?);
    Ok(encode_public_claim_reply(
        &claim,
        TimestampMillis::new(1000),
        CodecLimits::default().admission(),
        1_048_576,
    )?)
}
fn governed_caller() -> TestResult<NormalizedClaimDue<'static>> {
    let mut input = ClaimDueInput::new(b"r", b"worker");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::One(b"p");
    input.limit = Some(2);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(false);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    Ok(normalize_claim_due(input, ClaimCallerLimits::default())?)
}
#[test]
fn original_payment_has_one_canonical_source_batch_epoch_and_order_without_matching_new_attempt_nonce()
-> TestResult {
    let caller = governed_caller()?;
    let ids = [
        b"flr1:1:AAAAAAAAAAAAAAAAAAAAAA:1".to_vec(),
        b"flr1:1:AAAAAAAAAAAAAAAAAAAAAA:2".to_vec(),
    ];
    let body = paid_response(b"global", &ids)?;
    assert_eq!(
        inspect(&body, TimestampMillis::new(1000), &caller, profile())?
            .claim()
            .ok_or("claim")?
            .payment()
            .ok_or("original payment")?
            .count(),
        2
    );
    for second in [
        b"flr1:1:QQQQQQQQQQQQQQQQQQQQQQ:2".as_slice(),
        b"flr1:2:AAAAAAAAAAAAAAAAAAAAAA:2",
        b"flr1:1:AAAAAAAAAAAAAAAAAAAAAA:3",
    ] {
        let mixed = [ids[0].clone(), second.to_vec()];
        let body = paid_response(b"global", &mixed)?;
        assert!(inspect(&body, TimestampMillis::new(1000), &caller, profile()).is_err());
    }
    assert!(
        inspect(
            &paid_response(b"foreign", &ids)?,
            TimestampMillis::new(1000),
            &caller,
            profile()
        )
        .is_err()
    );
    let malformed = [
        b"flr1:1:AAAAAAAAAAAAAAAAAAAAAB:1".to_vec(),
        b"flr1:1:AAAAAAAAAAAAAAAAAAAAAB:2".to_vec(),
    ];
    assert!(
        inspect(
            &paid_response(b"global", &malformed)?,
            TimestampMillis::new(1000),
            &caller,
            profile()
        )
        .is_err()
    );
    Ok(())
}
