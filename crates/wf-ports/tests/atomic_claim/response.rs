//! Independent whole-parent literal grammar, including nested body validation.
use super::TestResult;
use wf_domain::{
    claim::apply::ClaimRejection,
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RetainedStatus},
    },
};
use wf_ports::log::{
    atomic_claim_retained_summary as summary, inspect_atomic_claim_reply as inspect,
};
use wf_types::version::TimestampMillis;

fn accepted() -> Vec<u8> {
    // One Requested phase containing the full existing empty FWCR1 body.
    let mut bytes = b"FWAR\0\x01\0\0\0\0\x01\0".to_vec();
    bytes.extend_from_slice(&21_u32.to_be_bytes());
    bytes.extend_from_slice(b"FWCR\0\x01\0\0\0");
    bytes.extend_from_slice(&[0; 12]);
    bytes
}

#[test]
fn parent_accepts_only_complete_original_nested_receipts() -> TestResult {
    let body = accepted();
    let limits = AdmissionLimits::default();
    let time = TimestampMillis::new(100);
    let reply = inspect(&body, time, limits)?;
    assert_eq!(
        (reply.status(), reply.count(), reply.phase_count()),
        (RetainedStatus::Accepted, 0, 1)
    );
    assert!(reply.used_indices().next().is_none());
    assert_eq!(
        summary(ClientIntent::atomic_claim([0; 32]), 17, &body, time, limits)?,
        (RetainedStatus::Accepted, Some(0))
    );
    for cut in 0..body.len() {
        assert!(inspect(&body[..cut], time, limits).is_err(), "cut {cut}");
    }
    let mut trailing = body.clone();
    trailing.push(0);
    assert!(inspect(&trailing, time, limits).is_err());
    let mut nested = body;
    nested[16] = b'X';
    assert!(inspect(&nested, time, limits).is_err());
    Ok(())
}

#[test]
fn parent_rejection_never_publishes_an_accepted_phase_prefix() -> TestResult {
    let mut bytes = b"FWAR\0\x01\0\x01".to_vec();
    bytes.extend_from_slice(&8_u32.to_be_bytes());
    bytes.extend_from_slice(b"FWCR\0\x01\x01\x01");
    let reply = inspect(
        &bytes,
        TimestampMillis::new(100),
        AdmissionLimits::default(),
    )?;
    assert_eq!(
        reply.status(),
        RetainedStatus::ClaimRejected(ClaimRejection::InvalidPriority)
    );
    assert_eq!((reply.count(), reply.phase_count()), (0, 0));
    assert!(reply.used_indices().next().is_none());
    bytes.extend_from_slice(&accepted());
    assert!(
        inspect(
            &bytes,
            TimestampMillis::new(100),
            AdmissionLimits::default()
        )
        .is_err()
    );
    Ok(())
}

fn nonempty_phase(partition: &[u8], id: &[u8], worker: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWCR\0\x01\0\0\0".to_vec();
    for number in [1_u32, 0, 1] {
        bytes.extend_from_slice(&number.to_be_bytes());
    }
    let mut token = worker.to_vec();
    token.extend_from_slice(b":100:2");
    for value in [partition, id, b"type", b"ready", worker, &token] {
        bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
        bytes.extend_from_slice(value);
    }
    for number in [3_u64, 2, 200, 1, 100, 0] {
        bytes.extend_from_slice(&number.to_be_bytes());
    }
    bytes.push(0);
    bytes.extend_from_slice(&1_u128.to_be_bytes());
    bytes.extend_from_slice(&[0; 3]);
    Ok(bytes)
}

fn two_phases(first: &[u8], second: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWAR\0\x01\0\0\0\0\x02".to_vec();
    for (kind, phase) in [(1_u8, first), (2_u8, second)] {
        bytes.push(kind);
        bytes.extend_from_slice(&u32::try_from(phase.len())?.to_be_bytes());
        bytes.extend_from_slice(phase);
    }
    Ok(bytes)
}

#[test]
fn parent_refuses_duplicate_workflow_identities_across_individually_valid_phases() -> TestResult {
    let first = nonempty_phase(b"p", b"id", b"worker")?;
    let body = two_phases(&first, &first)?;
    assert!(inspect(&body, TimestampMillis::new(100), AdmissionLimits::default()).is_err());
    Ok(())
}

#[test]
fn parent_requires_the_same_original_worker_across_nonempty_phases() -> TestResult {
    let first = nonempty_phase(b"p", b"first", b"worker")?;
    let second = nonempty_phase(b"p", b"second", b"other")?;
    let body = two_phases(&first, &second)?;
    assert!(inspect(&body, TimestampMillis::new(100), AdmissionLimits::default()).is_err());
    Ok(())
}

#[test]
fn same_record_id_in_distinct_partitions_is_not_a_duplicate() -> TestResult {
    let first = nonempty_phase(b"p", b"id", b"worker")?;
    let second = nonempty_phase(b"other", b"id", b"worker")?;
    let body = two_phases(&first, &second)?;
    let reply = inspect(&body, TimestampMillis::new(100), AdmissionLimits::default())?;
    assert_eq!((reply.count(), reply.phase_count()), (2, 2));
    Ok(())
}
