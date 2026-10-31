//! Private paid input identity and complete original wire admission.
use super::TestResult;
use wf_domain::{
    governance::release::LimitEnforcement,
    workflow::{admission::AdmissionLimits, request::RequestScope},
};
use wf_ports::{
    consensus::{PreparedClaimPhase, PreparedClaimQuery},
    log::{
        CodecError as E, CodecLimits, GovernedClaimPayment, PreparedBytes,
        encode_governed_claim_request, prepare_claim_phase, prepare_governed_claim_request,
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};

fn encoded(ids: &[Vec<u8>], epoch: u64, worker: &[u8]) -> TestResult<PreparedBytes> {
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    Ok(encode_governed_claim_request(
        &scope,
        &RequestId::new(b"claim".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(100, 0)?, 100)?,
        &PreparedClaimPhase {
            query: PreparedClaimQuery {
                workflow_type: b"type".to_vec(),
                states: Some(vec![b"q".to_vec()]),
                exclude_states: vec![],
                partitions: None,
                priority: None,
                worker: worker.to_vec(),
                lease_ms: 30,
                limit: 1,
            },
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 10,
            head_budget: 10,
        },
        GovernedClaimPayment {
            scope: b"scope\0",
            shard_id: 2,
            epoch,
            enforcement: LimitEnforcement::StrictGlobal,
            reservation_ids: ids,
        },
        limits(),
        1024 * 1024,
    )?)
}
fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

#[test]
fn paid_claim_semantic_intent_preserves_generated_id_epoch_independence() -> TestResult {
    let a = encoded(&[b"a\0".to_vec(), b"b".to_vec()], 1, b"w")?;
    let b = encoded(&[b"later".to_vec()], 2, b"w")?;
    let x = prepare_governed_claim_request(a.as_bytes(), limits(), 1024 * 1024)?;
    let y = prepare_governed_claim_request(b.as_bytes(), limits(), 1024 * 1024)?;
    assert_eq!(x.decoded().request().intent.version(), 19);
    assert_eq!(x.decoded().request().intent, y.decoded().request().intent);
    assert_ne!(x.capture().prepared(), y.capture().prepared());
    assert_eq!(
        x.capture()
            .payment()
            .reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        [b"a\0".as_slice(), b"b".as_slice()]
    );
    let other = encoded(&[b"a\0".to_vec(), b"b".to_vec()], 1, b"other")?;
    assert_ne!(
        x.decoded().request().intent,
        prepare_governed_claim_request(other.as_bytes(), limits(), 1024 * 1024)?
            .decoded()
            .request()
            .intent
    );
    assert!(prepare_claim_phase(a.as_bytes(), limits(), 1024 * 1024).is_err());
    Ok(())
}
#[test]
fn paid_claim_scans_complete_frame_and_refuses_duplicate_or_unpaid_ownership() -> TestResult {
    assert!(encoded(&[b"a".to_vec(), b"a".to_vec()], 1, b"w").is_err());
    let frame = encoded(&[b"a".to_vec()], 1, b"w")?;
    assert_eq!(
        prepare_governed_claim_request(frame.as_bytes(), limits(), 1).err(),
        Some(E::OwnedLimit)
    );
    for end in 0..frame.as_bytes().len() {
        assert!(
            prepare_governed_claim_request(&frame.as_bytes()[..end], limits(), 1024 * 1024)
                .is_err()
        );
    }
    let mut trailing = frame.as_bytes().to_vec();
    trailing.push(0);
    assert_eq!(
        prepare_governed_claim_request(&trailing, limits(), 1024 * 1024).err(),
        Some(E::TrailingBytes)
    );
    Ok(())
}

#[test]
fn short_paid_rejection_has_separate_scratch_and_closed_original_profile() -> TestResult {
    use wf_domain::{
        claim::apply::ClaimRejection as R,
        workflow::request::{ClientIntent, RetainedStatus},
    };
    use wf_ports::log::{
        claim_retained_summary, governed_claim_retained_summary, inspect_governed_claim_reply,
    };
    use wf_types::version::TimestampMillis;
    let mut reply = b"FWGC\0\x01".to_vec();
    reply.extend_from_slice(&1_u32.to_be_bytes());
    reply.push(b's');
    reply.extend_from_slice(&0_u64.to_be_bytes());
    reply.push(1);
    reply.extend_from_slice(&1_u64.to_be_bytes());
    reply.extend_from_slice(&2_u16.to_be_bytes());
    for id in *b"ab" {
        reply.extend_from_slice(&1_u32.to_be_bytes());
        reply.push(id);
    }
    reply.push(1);
    let admission = AdmissionLimits::new(1024, reply.len() + 102, 1000)?;
    let inspected = inspect_governed_claim_reply(&reply, TimestampMillis::new(100), admission)?;
    assert_eq!(inspected.used(), 0);
    assert_eq!(inspected.payment().count(), 2);
    let status = RetainedStatus::ClaimRejected(R::GoverningReservationUnavailable);
    assert_eq!(inspected.status(), status);
    assert_eq!(
        governed_claim_retained_summary(
            ClientIntent::governed_claim([1; 32]),
            16,
            &reply,
            TimestampMillis::new(100),
            admission
        )?,
        (status, None)
    );
    assert_eq!(
        governed_claim_retained_summary(
            ClientIntent::governed_claim([1; 32]),
            3,
            &reply,
            TimestampMillis::new(100),
            admission
        )
        .err(),
        Some(E::UnsupportedVersion)
    );
    assert!(
        claim_retained_summary(
            ClientIntent::new(1, [1; 32])?,
            3,
            &reply,
            TimestampMillis::new(100),
            admission
        )
        .is_err()
    );
    let end = reply.len() - 2;
    reply[end] = b'a';
    assert_eq!(
        inspect_governed_claim_reply(&reply, TimestampMillis::new(100), admission).err(),
        Some(E::Malformed)
    );
    Ok(())
}
