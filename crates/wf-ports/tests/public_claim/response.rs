//! Independent complete public-parent literal grammar, separate from commit proof.
use super::TestResult;
use wf_domain::{
    governance::{credit::CreditRejection, identified::IdentifiedCreditRejection as R},
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RetainedStatus as S},
    },
};
use wf_ports::log::{
    PublicClaimReplyKind as K, atomic_claim_retained_summary,
    inspect_public_claim_reply as inspect, public_claim_retained_summary as summary,
};
use wf_types::version::TimestampMillis;

fn empty_claim() -> Vec<u8> {
    let mut core = b"FWAR\0\x01\0\0\0\0\0".to_vec();
    let mut result = b"FWFR\0\x01\0".to_vec();
    result.extend_from_slice(&11_u32.to_be_bytes());
    result.append(&mut core);
    result
}

#[test]
fn public_parent_keeps_complete_actual_claim_receipt_and_separate_profile() -> TestResult {
    let response = empty_claim();
    let time = TimestampMillis::new(100);
    let admission = AdmissionLimits::default();
    let reply = inspect(&response, time, admission)?;
    assert_eq!((reply.kind(), reply.status()), (K::Claim, S::Accepted));
    assert_eq!(reply.claim().ok_or("claim")?.count(), 0);
    assert_eq!(
        summary(
            ClientIntent::public_claim([0; 32]),
            18,
            &response,
            time,
            admission
        )?,
        (S::Accepted, Some(0))
    );
    assert!(
        atomic_claim_retained_summary(
            ClientIntent::public_claim([0; 32]),
            18,
            &response,
            time,
            admission
        )
        .is_err()
    );
    assert!(
        summary(
            ClientIntent::atomic_claim([0; 32]),
            18,
            &response,
            time,
            admission
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn physical_no_demand_is_distinct_from_both_empty_selection_and_funding_rejection() -> TestResult {
    let empty = b"FWFR\0\x01\x01\x01";
    let rejected = b"FWFR\0\x01\x02\x01";
    let admission = AdmissionLimits::default();
    let time = TimestampMillis::new(100);
    let no_demand = inspect(empty, time, admission)?;
    assert_eq!(
        (
            no_demand.kind(),
            no_demand.status(),
            no_demand.failed_credit()
        ),
        (K::NoDemand, S::Accepted, Some(R::NotFound))
    );
    assert!(no_demand.claim().is_none());
    let failure = inspect(rejected, time, admission)?;
    assert_eq!(
        (failure.kind(), failure.status()),
        (K::FundingRejected, S::GovernanceRejected(R::NotFound))
    );
    assert_eq!(
        summary(
            ClientIntent::public_claim([0; 32]),
            18,
            rejected,
            time,
            admission
        )?,
        (S::GovernanceRejected(R::NotFound), None)
    );
    Ok(())
}

#[test]
fn funding_rejection_preserves_the_original_quantities_and_requires_all_bytes() -> TestResult {
    let mut response = b"FWFR\0\x01\x02\0\x01".to_vec();
    for number in [4_u64, 0, 1] {
        response.extend_from_slice(&number.to_be_bytes());
    }
    let time = TimestampMillis::new(100);
    let admission = AdmissionLimits::default();
    assert_eq!(
        inspect(&response, time, admission)?.status(),
        S::GovernanceRejected(R::Credit(CreditRejection::LimitExceeded {
            requested: 4,
            free: 0,
            limit: 1
        }))
    );
    for end in 0..response.len() {
        assert!(
            inspect(&response[..end], time, admission).is_err(),
            "truncated {end}"
        );
    }
    response.push(0);
    assert!(inspect(&response, time, admission).is_err());
    Ok(())
}
