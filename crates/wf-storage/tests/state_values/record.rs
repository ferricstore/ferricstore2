use super::TestResult;
use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::{
    CommittedCommand, Create, MAX_EXACT_MILLIS, Operation, Terminal, WorkflowRecord, reduce,
};
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::state::value::{StateValueError, decode_record, encode_record};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

// Independently packed before production: all seven names contain NUL/FF,
// priority2, exact u128::MAX-7 sequence, due7 and present-empty payload.
const RECORD: &str = "465753520001000000036700ff000000037a00ff000000037400ff000000037000ff02fffffffffffffffffffffffffffffff8000000036900ff000000037200ff000000037100ff000000000000000100000000000000000000000000000000000000000000000100000000000000010100000000000000070000010000000000000000000000000000010000000000000000";

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn changed(
    previous: Option<&WorkflowRecord>,
    now: u64,
    operation: Operation,
) -> TestResult<WorkflowRecord> {
    let plan = reduce(
        previous,
        &CommittedCommand {
            id: RecordId::new(b"i\0\xff".to_vec())?,
            now: TimestampMillis::new(now),
            operation,
        },
    )?;
    let history = plan.history().ok_or("changed history absent")?;
    let record = &plan.write().ok_or("changed record absent")?.record;
    assert_eq!(
        record.latest_event_witness(),
        (history.kind, history.version, history.fence)
    );
    Ok(plan.into_parts().1.ok_or("changed owner absent")?.record)
}

fn created() -> TestResult<WorkflowRecord> {
    changed(
        None,
        1,
        Operation::Create(Create {
            workflow_type: b"r\0\xff".to_vec(),
            state: b"q\0\xff".to_vec(),
            run_at: Some(TimestampMillis::new(7)),
            payload: Some(Vec::new()),
        }),
    )
}

fn stored(record: WorkflowRecord) -> TestResult<StoredWorkflowRecord> {
    Ok(StoredWorkflowRecord {
        tenant: TenantId::new(b"t\0\xff".to_vec())?,
        partition: PartitionId::new(b"p\0\xff".to_vec())?,
        record,
        ordering: OrderingMetadata {
            priority: 2,
            state_enter_seq: u128::MAX - 7,
        },
    })
}

fn claim() -> TestResult<Operation> {
    Ok(Operation::Claim(Claim {
        expected_state: b"q\0\xff".to_vec(),
        worker: b"w\0\xff".to_vec(),
        token: LeaseToken::new(b"tok\0\xff".to_vec())?,
        lease_ms: 1,
    }))
}

fn guard(record: &WorkflowRecord) -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: record.lease().ok_or("claimed lease absent")?.token.clone(),
        fence: record.fence(),
    })
}

fn roundtrip(record: WorkflowRecord, expected_kind: u8) -> TestResult {
    let group = GroupId::new(b"g\0\xff".to_vec())?;
    let recovery = RecoveryId::new(b"z\0\xff".to_vec())?;
    let owner = stored(record)?;
    let limits = AdmissionLimits::default();
    let maximum = limits
        .decision_bytes()
        .checked_add(131_072)
        .ok_or("cap overflow")?;
    let encoded = encode_record(&group, &recovery, &owner, limits, maximum)?;
    assert!(encoded.len() <= maximum);
    assert_eq!(encoded[encoded.len() - 17], expected_kind);
    assert_eq!(
        decode_record(&encoded, &group, &recovery, limits, maximum)?,
        owner
    );
    Ok(())
}

#[test]
fn core_record_vectors_roundtrip_reducer_and_latest_witness() -> TestResult {
    let group = GroupId::new(b"g\0\xff".to_vec())?;
    let recovery = RecoveryId::new(b"z\0\xff".to_vec())?;
    let limits = AdmissionLimits::default();
    let initial = created()?;
    let owner = stored(initial.clone())?;
    let expected = hex(RECORD)?;
    assert_eq!(expected.len(), 147);
    assert_eq!(
        encode_record(&group, &recovery, &owner, limits, 147)?,
        expected
    );
    assert_eq!(
        decode_record(&expected, &group, &recovery, limits, 147)?,
        owner
    );
    assert_eq!(
        owner.record.latest_event_witness(),
        (EventKind::Created, 1, Fence::new(0))
    );
    assert_eq!(owner.record.payload(), Some(b"".as_slice()));
    assert_eq!(owner.record.result(), None);
    assert_eq!(owner.record.error(), None);
    let running = changed(Some(&initial), 7, claim()?)?;
    assert_eq!(
        running.latest_event_witness(),
        (EventKind::Claimed, 2, Fence::new(1))
    );
    assert_eq!(running.run_state(), Some(b"q\0\xff".as_slice()));
    roundtrip(running.clone(), 1)?;
    let completed = changed(
        Some(&running),
        8,
        Operation::Complete(Terminal {
            guard: guard(&running)?,
            payload: Some(Vec::new()),
            value: Some(Vec::new()),
        }),
    )?;
    assert_eq!(
        completed.latest_event_witness(),
        (EventKind::Completed, 3, Fence::new(1))
    );
    assert_eq!(completed.result(), Some(b"".as_slice()));
    assert_eq!(completed.next_run_at(), None);
    roundtrip(completed, 3)?;
    let late = changed(Some(&initial), MAX_EXACT_MILLIS - 100, claim()?)?;
    let retried = changed(
        Some(&late),
        MAX_EXACT_MILLIS - 100,
        Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(&late)?,
            run_at: None,
            policy_override: RetryOverride {
                backoff: Some(Backoff {
                    kind: BackoffKind::Fixed,
                    base_ms: 200,
                    max_ms: 200,
                    jitter_pct: 0,
                }),
                ..RetryOverride::default()
            },
            jitter: JitterSample::new(0),
            payload: Some(Vec::new()),
            error: Some(Vec::new()),
        }),
    )?;
    assert_eq!(
        retried.next_run_at(),
        Some(TimestampMillis::new(MAX_EXACT_MILLIS + 100))
    );
    assert_eq!(
        retried.latest_event_witness(),
        (EventKind::Retry, 3, Fence::new(1))
    );
    assert_eq!(retried.attempts(), 1);
    assert_eq!(retried.error(), Some(b"".as_slice()));
    roundtrip(retried, 5)
}

#[test]
fn record_corruption_and_limits_preserve_no_partial_owner() -> TestResult {
    let group = GroupId::new(b"g\0\xff".to_vec())?;
    let recovery = RecoveryId::new(b"z\0\xff".to_vec())?;
    let limits = AdmissionLimits::default();
    let expected = hex(RECORD)?;
    let owner = stored(created()?)?;
    for end in 0..expected.len() {
        assert!(decode_record(&expected[..end], &group, &recovery, limits, 147).is_err());
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert_eq!(
        decode_record(&trailing, &group, &recovery, limits, 148),
        Err(StateValueError::Malformed)
    );
    for offset in [112, 121, 122, 123, 128, 129, 130] {
        let mut bad = expected.clone();
        bad[offset] = if offset == 130 { 13 } else { 2 };
        assert_eq!(
            decode_record(&bad, &group, &recovery, limits, 147),
            Err(StateValueError::UnsupportedTag)
        );
    }
    let mut version = expected.clone();
    version[5] = 2;
    assert_eq!(
        decode_record(&version, &group, &recovery, limits, 147),
        Err(StateValueError::UnsupportedVersion)
    );
    let mut bad_magic = expected.clone();
    bad_magic[0] = b'X';
    assert_eq!(
        decode_record(&bad_magic, &group, &recovery, limits, 147),
        Err(StateValueError::UnsupportedTag)
    );
    for offset in [34, 131] {
        let mut bad = expected.clone();
        bad[offset] = if offset == 34 { 3 } else { 1 };
        assert_eq!(
            decode_record(&bad, &group, &recovery, limits, 147),
            Err(StateValueError::InvalidValue)
        );
    }
    let foreign = GroupId::new(b"foreign".to_vec())?;
    assert_eq!(
        decode_record(&expected, &foreign, &recovery, limits, 147),
        Err(StateValueError::InvalidValue)
    );
    let foreign_recovery = RecoveryId::new(b"other".to_vec())?;
    assert_eq!(
        decode_record(&expected, &group, &foreign_recovery, limits, 147),
        Err(StateValueError::InvalidValue)
    );
    let tight = StateValueError::Limit {
        bytes: 147,
        maximum: 146,
    };
    assert_eq!(
        encode_record(&group, &recovery, &owner, limits, 146),
        Err(tight)
    );
    assert_eq!(
        decode_record(&expected, &group, &recovery, limits, 146),
        Err(tight)
    );
    assert_eq!(
        decode_record(&expected, &group, &recovery, limits, 0),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        encode_record(&group, &recovery, &owner, limits, 0),
        Err(StateValueError::InvalidValue)
    );
    let tiny = AdmissionLimits::new(1, 8, 1)?;
    assert_eq!(
        encode_record(&group, &recovery, &owner, tiny, 147),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        decode_record(&expected, &group, &recovery, tiny, 147),
        Err(StateValueError::InvalidValue)
    );
    let mut wrong_priority = owner.clone();
    wrong_priority.ordering.priority = 3;
    assert_eq!(
        encode_record(&group, &recovery, &wrong_priority, limits, 147),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(owner.ordering.priority, 2);
    assert_eq!(owner.record, created()?);
    Ok(())
}
