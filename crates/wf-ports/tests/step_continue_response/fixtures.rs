use super::*;
use std::collections::BTreeMap;
use wf_domain::claim::{Claim, ClaimRecord, DueOrder, ScopedRecordId};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::batch::{BatchItem, GroupView};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexedBatchItem, IndexedGroupView, decide_indexed_batch,
};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, StepContinue, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};

pub(super) const GOLDEN: &str = concat!(
    "4657535400010000000000000001000000000000000700000000000000090000000000000002",
    "000000000170000000016900000005656d61696c00000006636861726765000000037700ff",
    "000000077700ff3a323a32000000000000000300000000000000020000000000007532",
    "00000000000000000000000000000002000000000000000101000000000000000000000009",
    "00000003010000000200ff0000"
);

pub(super) fn limits() -> TestResult<AdmissionLimits> {
    Ok(AdmissionLimits::new(32, 8192, 4)?)
}

pub(super) const fn position() -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(9))
}

pub(super) const fn time() -> TimestampMillis {
    TimestampMillis::new(2)
}

fn running() -> TestResult<WorkflowRecord> {
    let create = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"email".to_vec(),
            state: b"authorize".to_vec(),
            run_at: None,
            payload: Some(b"old".to_vec()),
        }),
    };
    let (_, Some(write), _) = reduce(None, &create)?.into_parts() else {
        return Err("fixture creation".into());
    };
    let record = write.record;
    let claim = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Claim(Claim {
            expected_state: b"authorize".to_vec(),
            worker: b"w\0\xff".to_vec(),
            token: LeaseToken::new(b"lease".to_vec())?,
            lease_ms: 30_000,
        }),
    };
    let (_, Some(write), _) = reduce(Some(&record), &claim)?.into_parts() else {
        return Err("fixture claim".into());
    };
    Ok(write.record)
}

pub(super) fn with_step<T>(
    to: &[u8],
    payload: Option<&[u8]>,
    check: impl FnOnce(&IndexedDecision<'_>) -> TestResult<T>,
) -> TestResult<T> {
    let record = running()?;
    with_record(&record, to, payload, check)
}

pub(super) fn with_legacy_step<T>(
    check: impl FnOnce(&IndexedDecision<'_>) -> TestResult<T>,
) -> TestResult<T> {
    let record = running()?;
    let record = restore_record(
        WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: b"running".to_vec(),
            version: 1,
            fence: wf_types::version::Fence::new(0),
            attempts: 0,
            created_at: TimestampMillis::new(7),
            updated_at: TimestampMillis::new(1),
            next_run_at: record.next_run_at(),
            run_state: Some(b"authorize".to_vec()),
            lease: record.lease().cloned(),
            payload: Some(b"old".to_vec()),
            result: Some(b"res\0\xff".to_vec()),
            error: Some(Vec::new()),
            latest_kind: wf_domain::history::EventKind::Claimed,
            latest_version: 1,
            latest_fence: wf_types::version::Fence::new(0),
        },
        AdmissionLimits::default(),
    )?;
    with_record(&record, b"charge", None, check)
}

fn with_record<T>(
    record: &WorkflowRecord,
    to: &[u8],
    payload: Option<&[u8]>,
    check: impl FnOnce(&IndexedDecision<'_>) -> TestResult<T>,
) -> TestResult<T> {
    let entry = ClaimRecord {
        record,
        partition_key: b"p",
        priority: 1,
        state_enter_seq: 5,
    };
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let id = ScopedRecordId::for_record(&entry);
    let canonical = BTreeMap::from([(id, record)]);
    let records = BTreeMap::from([(id, entry)]);
    let due = DueOrder::for_record(&entry).ok_or("fixture due")?;
    let parallel = BTreeMap::from([(due, entry)]);
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let lease = record.lease().ok_or("fixture lease")?;
    let command = CommittedCommand {
        id: record.id().clone(),
        now: time(),
        operation: Operation::StepContinue(StepContinue {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: record.fence(),
            },
            from: b"authorize".to_vec(),
            to: to.to_vec(),
            worker: None,
            lease_ms: 30_000,
            payload: payload.map(<[u8]>::to_vec),
        }),
    };
    let items = [IndexedBatchItem::new(BatchItem::new(
        &partition,
        &command,
        b"state-key",
    ))];
    let decision = decide_indexed_batch(
        IndexedGroupView {
            batch: GroupView {
                group: &group,
                placements: &placements,
                records: &canonical,
            },
            records: &records,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
        &items,
        EntryOrder {
            identity: position(),
            first_ordinal: 3,
        },
        b"prepared",
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(decision, IndexedDecision::Accepted(_)),
        "fixture decision: {decision:?}"
    );
    check(&decision)
}

pub(super) fn reply() -> TestResult<Vec<u8>> {
    with_step(b"charge", Some(b"\0\xff"), |decision| {
        let result = encode_step_continue_response(decision, position(), time(), limits()?);
        assert!(result.is_ok(), "healthy Step reply: {result:?}");
        let (bytes, status) = result?;
        assert_eq!(status, RetainedStatus::Accepted);
        Ok(bytes)
    })
}

pub(super) fn summary(bytes: &[u8]) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    step_continue_retained_summary(
        ClientIntent::step_continue([8; 32]),
        9,
        bytes,
        position(),
        time(),
        AdmissionLimits::new(32, 8192, 4).map_err(|_| CodecError::Malformed)?,
    )
}

pub(super) fn hex(value: &str) -> TestResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd fixture hex".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
