//! Independent FWSH vectors; codec coverage does not establish engine durability.

use super::TestResult;
use wf_domain::claim::Claim;
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry, RetryDecision};
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_storage::state::value::{StateValueError, decode_history, encode_history};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

// Packed independently before the SUT: create at1 and claim10/retry8 with due20.
const CREATED: &str = "4657534800010000000167000000017a00000001740000000170000000016900000003312d31000000000000000001000000000000000100000000000000000000000000000000000000017101000000000000000100000000000000000000";
const RETRIED: &str = "4657534800010000000167000000017a0000000174000000017000000001690000000431302d33050000000000000008000000000000000300000000000000010000000000000001000000017101000000000000001400000000000000000001000000000171010000000000000014000301000000000000000a000000000000001e00000000066661696c6564";

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn scope(tenant: &[u8], group: &[u8], recovery: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(tenant.to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(recovery.to_vec())?,
    ))
}

fn changed(
    previous: Option<&WorkflowRecord>,
    now: u64,
    operation: Operation,
) -> TestResult<(WorkflowRecord, HistoryEntry)> {
    let (_, write, history) = reduce(
        previous,
        &CommittedCommand {
            id: RecordId::new(b"i".to_vec())?,
            now: TimestampMillis::new(now),
            operation,
        },
    )?
    .into_parts();
    Ok((
        write.ok_or("changed record absent")?.record,
        history.ok_or("changed history absent")?,
    ))
}

fn created() -> TestResult<(WorkflowRecord, HistoryEntry)> {
    changed(
        None,
        1,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: None,
        }),
    )
}

fn retry(exhausted: bool) -> TestResult<HistoryEntry> {
    let (initial, _) = created()?;
    let (running, claimed) = changed(
        Some(&initial),
        10,
        Operation::Claim(Claim {
            expected_state: b"q".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"k".to_vec())?,
            lease_ms: 1,
        }),
    )?;
    assert_eq!(claimed.event_id, "10-2");
    assert_eq!(claimed.worker.as_deref(), Some(b"w".as_slice()));
    assert_eq!(claimed.lease_deadline, TimestampMillis::new(11));
    assert_eq!(claimed.next_run_at, Some(TimestampMillis::new(11)));
    roundtrip(&claimed)?;
    let (_, history) = changed(
        Some(&running),
        8,
        Operation::RetryWithPolicy(PolicyRetry {
            guard: LeaseGuard {
                token: running.lease().ok_or("claim lease absent")?.token.clone(),
                fence: running.fence(),
            },
            run_at: Some(TimestampMillis::new(20)),
            policy_override: RetryOverride {
                max_retries: Some(if exhausted { 0 } else { 3 }),
                backoff: Some(Backoff {
                    kind: BackoffKind::Fixed,
                    base_ms: 10,
                    max_ms: 30,
                    jitter_pct: 0,
                }),
                exhausted_to: None,
            },
            jitter: JitterSample::new(0),
            payload: None,
            error: None,
        }),
    )?;
    assert_eq!(history.event_id, "10-3");
    assert_eq!(history.occurred_at, TimestampMillis::new(8));
    assert_eq!(history.version, 3);
    assert_eq!(history.fence, Fence::new(1));
    assert_eq!(history.attempts, 1);
    Ok(history)
}

fn roundtrip(history: &HistoryEntry) -> TestResult<Vec<u8>> {
    let scope = scope(b"t", b"g", b"z")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let encoded = encode_history(&scope, &partition, &id, history, 65_536)?;
    let donor = encoded.clone();
    let restored = decode_history(&donor, &scope, &partition, &id, donor.len())?;
    drop(donor);
    assert_eq!(&restored, history);
    Ok(encoded)
}

fn mathematical_variants(history: &HistoryEntry) -> TestResult {
    // Mathematical scalar/Option preservation, not thirteen reducer scenarios.
    for (tag, kind) in [
        EventKind::Created,
        EventKind::Claimed,
        EventKind::Transitioned,
        EventKind::Completed,
        EventKind::Failed,
        EventKind::Retry,
        EventKind::LeaseExtended,
        EventKind::Cancelled,
        EventKind::ChildCompleted,
        EventKind::ChildFailed,
        EventKind::ChildCancelled,
        EventKind::ChildrenCancelled,
        EventKind::ChildrenSpawned,
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = history.clone();
        entry.kind = kind;
        assert_eq!(usize::from(roundtrip(&entry)?[39]), tag);
    }
    for decision in [RetryDecision::Scheduled, RetryDecision::Exhausted] {
        for (tag, kind) in [
            BackoffKind::None,
            BackoffKind::Fixed,
            BackoffKind::Linear,
            BackoffKind::Exponential,
        ]
        .into_iter()
        .enumerate()
        {
            let mut entry = history.clone();
            let audit = entry.retry.as_mut().ok_or("retry audit absent")?;
            audit.decision = decision;
            audit.policy.backoff.kind = kind;
            let bytes = roundtrip(&entry)?;
            assert_eq!(bytes[96], u8::from(decision == RetryDecision::Exhausted));
            assert_eq!(usize::from(bytes[113]), tag);
        }
    }
    for version in [0, (1_u64 << 53) + 1, u64::MAX] {
        let mut entry = history.clone();
        entry.event_id = format!("{}-{version}", u64::MAX);
        entry.version = version;
        entry.occurred_at = TimestampMillis::new(0);
        entry.fence = Fence::new(u64::MAX);
        entry.attempts = u64::MAX;
        entry.lease_deadline = TimestampMillis::new(u64::MAX);
        entry.next_run_at = Some(TimestampMillis::new(u64::MAX));
        roundtrip(&entry)?;
    }
    let mut entry = history.clone();
    entry.state.clear();
    entry.worker = Some(Vec::new());
    entry.next_run_at = None;
    let audit = entry.retry.as_mut().ok_or("retry audit absent")?;
    audit.run_state.clear();
    audit.next_run_at = Some(TimestampMillis::new(999));
    roundtrip(&entry)?;
    entry.worker = Some(b"w\0\xff".to_vec());
    entry.state = b"q\0\xff".to_vec();
    let audit = entry.retry.as_mut().ok_or("retry audit absent")?;
    audit.run_state = b"r\0\xff".to_vec();
    audit.next_run_at = None;
    audit.policy.max_retries = 1_000;
    audit.policy.backoff.base_ms = 2_592_000_000;
    audit.policy.backoff.max_ms = 2_592_000_000;
    audit.policy.backoff.jitter_pct = 100;
    audit.policy.exhausted_to = b"q\0\xff".to_vec();
    entry.next_run_at = Some(TimestampMillis::new(42));
    roundtrip(&entry)?;
    Ok(())
}

#[test]
fn history_vectors_preserve_clock_and_retry_audit() -> TestResult {
    let expected_scope = scope(b"t", b"g", b"z")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let (_, initial) = created()?;
    let retried = retry(false)?;
    for (history, literal, length) in [(&initial, CREATED, 95), (&retried, RETRIED, 141)] {
        let expected = hex(literal)?;
        assert_eq!(expected.len(), length);
        assert_eq!(
            encode_history(&expected_scope, &partition, &id, history, length)?,
            expected
        );
        assert_eq!(
            decode_history(&expected, &expected_scope, &partition, &id, length)?,
            *history
        );
        assert_eq!(roundtrip(history)?, expected);
    }
    let audit = retried.retry.as_ref().ok_or("retry audit absent")?;
    assert_eq!(audit.decision, RetryDecision::Scheduled);
    assert_eq!(audit.run_state, b"q");
    assert_eq!(audit.next_run_at, Some(TimestampMillis::new(20)));
    assert_eq!(retried.next_run_at, audit.next_run_at);
    let exhausted = retry(true)?;
    let audit = exhausted.retry.as_ref().ok_or("exhausted audit absent")?;
    assert_eq!(audit.decision, RetryDecision::Exhausted);
    assert_eq!(audit.policy.max_retries, 0);
    assert_eq!(audit.policy.exhausted_to, b"failed");
    assert_eq!(audit.next_run_at, None);
    assert_eq!(exhausted.next_run_at, None);
    assert_eq!(exhausted.state, b"failed");
    roundtrip(&exhausted)?;
    let binary_scope = scope(b"t\0\xff", b"g\0\xff", b"z\0\xff")?;
    let binary_partition = PartitionId::new(b"p\0\xff".to_vec())?;
    let binary_id = RecordId::new(b"i\0\xff".to_vec())?;
    let donor = encode_history(
        &binary_scope,
        &binary_partition,
        &binary_id,
        &retried,
        65_536,
    )?;
    let owner = decode_history(
        &donor,
        &binary_scope,
        &binary_partition,
        &binary_id,
        donor.len(),
    )?;
    drop(donor);
    assert_eq!(owner, retried);
    mathematical_variants(&retried)
}

fn rejected(bytes: &[u8], expected: StateValueError) -> TestResult {
    assert_eq!(
        decode_history(
            bytes,
            &scope(b"t", b"g", b"z")?,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
            bytes.len().max(1),
        ),
        Err(expected)
    );
    Ok(())
}

fn with_event_id(bytes: &[u8], event_id: &[u8]) -> TestResult<Vec<u8>> {
    // Five one-byte namespace fields end at31; event-ID framing is independent.
    let old_length = usize::try_from(u32::from_be_bytes(bytes[31..35].try_into()?))?;
    let mut result = bytes[..31].to_vec();
    result.extend_from_slice(&u32::try_from(event_id.len())?.to_be_bytes());
    result.extend_from_slice(event_id);
    result.extend_from_slice(&bytes[35 + old_length..]);
    Ok(result)
}

fn malformed_history(created: &[u8], retried: &[u8]) -> TestResult {
    for bytes in [created, retried] {
        for end in 0..bytes.len() {
            rejected(&bytes[..end], StateValueError::Malformed)?;
        }
        let mut trailing = bytes.to_vec();
        trailing.push(0);
        rejected(&trailing, StateValueError::Malformed)?;
    }
    for (offset, value, expected) in [
        (0, b'X', StateValueError::UnsupportedTag),
        (5, 2, StateValueError::UnsupportedVersion),
        (38, 13, StateValueError::UnsupportedTag),
        (76, 2, StateValueError::UnsupportedTag),
        (93, 2, StateValueError::UnsupportedTag),
        (94, 2, StateValueError::UnsupportedTag),
        (54, 2, StateValueError::InvalidValue),
    ] {
        let mut bad = created.to_vec();
        bad[offset] = value;
        rejected(&bad, expected)?;
    }
    for offset in [94, 95, 96, 102, 113] {
        let mut bad = retried.to_vec();
        bad[offset] = if offset == 113 { 4 } else { 2 };
        rejected(&bad, StateValueError::UnsupportedTag)?;
    }
    for (bytes, offsets) in [
        (created, [6, 11, 16, 21, 26, 31, 71].as_slice()),
        (retried, [6, 11, 16, 21, 26, 31, 72, 97, 131].as_slice()),
    ] {
        for &offset in offsets {
            let mut bad = bytes.to_vec();
            bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            rejected(&bad, StateValueError::Malformed)?;
        }
    }
    for event_id in [
        b"".as_slice(),
        b"1",
        b"-1",
        b"1-",
        b"01-1",
        b"1-01",
        b"+1-1",
        b"1-+1",
        b"1--1",
        b"1-1-1",
        b"1-1\0",
        b"1-\xff",
        b"18446744073709551616-1",
        b"1-18446744073709551616",
    ] {
        rejected(
            &with_event_id(created, event_id)?,
            StateValueError::InvalidValue,
        )?;
    }
    Ok(())
}

fn invalid_policies(history: &HistoryEntry, retried: &[u8]) -> TestResult {
    let scope = scope(b"t", b"g", b"z")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    for (offset, replacement) in [
        (111, 1001_u16.to_be_bytes().to_vec()),
        (114, 2_592_000_001_u64.to_be_bytes().to_vec()),
        (122, 2_592_000_001_u64.to_be_bytes().to_vec()),
        (130, vec![101]),
    ] {
        let mut bad = retried.to_vec();
        bad[offset..offset + replacement.len()].copy_from_slice(&replacement);
        rejected(&bad, StateValueError::InvalidValue)?;
    }
    for target in [b"".as_slice(), b"running"] {
        let mut bad = retried[..131].to_vec();
        bad.extend_from_slice(&u32::try_from(target.len())?.to_be_bytes());
        bad.extend_from_slice(target);
        rejected(&bad, StateValueError::InvalidValue)?;
    }
    for field in 0..6 {
        let mut bad = history.clone();
        let audit = bad.retry.as_mut().ok_or("retry audit absent")?;
        match field {
            0 => audit.policy.max_retries = 1001,
            1 => audit.policy.backoff.base_ms = 2_592_000_001,
            2 => audit.policy.backoff.max_ms = 2_592_000_001,
            3 => audit.policy.backoff.jitter_pct = 101,
            4 => audit.policy.exhausted_to.clear(),
            _ => audit.policy.exhausted_to = b"running".to_vec(),
        }
        assert_eq!(
            encode_history(&scope, &partition, &id, &bad, 65_536),
            Err(StateValueError::InvalidValue)
        );
    }
    Ok(())
}

fn namespace_and_caps(history: &HistoryEntry, bytes: &[u8]) -> TestResult {
    let expected_scope = scope(b"t", b"g", b"z")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    for foreign in [
        scope(b"other", b"g", b"z")?,
        scope(b"t", b"other", b"z")?,
        scope(b"t", b"g", b"other")?,
    ] {
        assert_eq!(
            decode_history(bytes, &foreign, &partition, &id, bytes.len()),
            Err(StateValueError::InvalidValue)
        );
    }
    assert_eq!(
        decode_history(
            bytes,
            &expected_scope,
            &PartitionId::new(b"other".to_vec())?,
            &id,
            bytes.len()
        ),
        Err(StateValueError::InvalidValue)
    );
    assert_eq!(
        decode_history(
            bytes,
            &expected_scope,
            &partition,
            &RecordId::new(b"other".to_vec())?,
            bytes.len()
        ),
        Err(StateValueError::InvalidValue)
    );
    for maximum in [0, bytes.len() - 1] {
        let expected = if maximum == 0 {
            StateValueError::InvalidValue
        } else {
            StateValueError::Limit {
                bytes: bytes.len(),
                maximum,
            }
        };
        assert_eq!(
            encode_history(&expected_scope, &partition, &id, history, maximum),
            Err(expected)
        );
        assert_eq!(
            decode_history(bytes, &expected_scope, &partition, &id, maximum),
            Err(expected)
        );
    }
    let mut bad = bytes.to_vec();
    bad[0] = b'X';
    assert_eq!(
        decode_history(&bad, &expected_scope, &partition, &id, bytes.len() - 1),
        Err(StateValueError::Limit {
            bytes: bytes.len(),
            maximum: bytes.len() - 1
        })
    );
    let mut bad = bytes.to_vec();
    bad.push(0);
    assert_eq!(
        decode_history(
            &bad,
            &scope(b"foreign", b"g", b"z")?,
            &partition,
            &id,
            bad.len()
        ),
        Err(StateValueError::Malformed)
    );
    for event_id in ["01-1", "1-2", "1-18446744073709551616"] {
        let mut bad = history.clone();
        event_id.clone_into(&mut bad.event_id);
        assert_eq!(
            encode_history(&expected_scope, &partition, &id, &bad, 65_536),
            Err(StateValueError::InvalidValue)
        );
    }
    Ok(())
}

#[test]
fn history_corruption_and_caps_fail_before_owners() -> TestResult {
    let created_bytes = hex(CREATED)?;
    let retried_bytes = hex(RETRIED)?;
    malformed_history(&created_bytes, &retried_bytes)?;
    namespace_and_caps(&created()?.1, &created_bytes)?;
    namespace_and_caps(&retry(false)?, &retried_bytes)?;
    invalid_policies(&retry(false)?, &retried_bytes)
}
