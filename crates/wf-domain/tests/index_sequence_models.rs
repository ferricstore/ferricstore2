//! Supplemental ordering oracle for claim/reclaim/retry and legacy duplicates.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, ScopedRecordId};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{BatchItem, GroupView};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexRows, IndexedBatchItem, IndexedDecision, IndexedGroupView,
    decide_indexed_batch,
};
use wf_domain::workflow::{CommittedCommand, Create, Operation, Outcome, WorkflowRecord, reduce};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn command(record: &WorkflowRecord, now: u64, operation: Operation) -> CommittedCommand {
    CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(now),
        operation,
    }
}

fn next(
    record: Option<&WorkflowRecord>,
    command: &CommittedCommand,
) -> Result<WorkflowRecord, TestError> {
    Ok(reduce(record, command)?
        .into_parts()
        .1
        .ok_or("fixture write absent")?
        .record)
}

fn assert_preserved(
    record: &WorkflowRecord,
    command: &CommittedCommand,
    outcome: Outcome,
) -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let entry = ClaimRecord {
        record,
        partition_key: partition.as_bytes(),
        priority: 2,
        state_enter_seq: 99,
    };
    let records = BTreeMap::from([(ScopedRecordId::for_record(&entry), record)]);
    let current = BTreeMap::from([(ScopedRecordId::for_record(&entry), entry)]);
    let parallel = BTreeMap::from([(
        DueOrder::for_record(&entry).ok_or("fixture due absent")?,
        entry,
    )]);
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let view = IndexedGroupView {
        batch: GroupView {
            group: &group,
            placements: &placements,
            records: &records,
        },
        records: &current,
        parallel: &parallel,
        fifo: &fifo,
        policies: &policies,
    };
    let item = IndexedBatchItem::new(BatchItem::new(&partition, command, b"key"));
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(25)),
        first_ordinal: u32::MAX,
    };
    let decision =
        decide_indexed_batch(view, &[item], order, b"entry", AdmissionLimits::default())?;
    assert_eq!(
        decision,
        decide_indexed_batch(view, &[item], order, b"entry", AdmissionLimits::default())?
    );
    let IndexedDecision::Accepted(plan) = decision else {
        return Err("ordering command rejected".into());
    };
    assert_eq!(plan.consumed_ordinals(), 0);
    assert_eq!(plan.next_ordinal(), Some(u32::MAX));
    let metadata = plan.metadata().next().ok_or("ordering metadata absent")?;
    assert_eq!(metadata.state_enter_seq, 99);
    assert_eq!(metadata.priority, 2);
    let record_plan = plan.batch().items()[0].plan();
    assert_eq!(record_plan.outcome(), outcome);
    let delta = plan.deltas().next().ok_or("delta absent")?;
    if outcome == Outcome::LegacyNoop {
        assert!(record_plan.write().is_none() && record_plan.history().is_none());
        assert!(delta.before.is_none() && delta.after.is_none());
    } else {
        assert!(
            matches!(delta.before, Some(IndexRows::Parallel { due }) if due.priority == 2 && due.state == b"queued")
        );
        assert!(
            matches!(delta.after, Some(IndexRows::Parallel { due }) if due.priority == 2 && due.state == b"queued")
        );
    }
    Ok(())
}

#[test]
fn unchanged_logical_state_preserves_sequence_even_with_exhausted_ordinal() -> TestResult {
    // flow_create.ex:327-341 uses logical state, including running's run_state.
    let created = next(
        None,
        &CommittedCommand {
            id: RecordId::new(b"id".to_vec())?,
            now: TimestampMillis::new(0),
            operation: Operation::Create(Create {
                workflow_type: b"job".to_vec(),
                state: b"queued".to_vec(),
                run_at: None,
                payload: None,
            }),
        },
    )?;
    let token = LeaseToken::new(b"claim-token".to_vec())?;
    let claim = command(
        &created,
        10,
        Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"w".to_vec(),
            token: token.clone(),
            lease_ms: 10,
        }),
    );
    assert_preserved(&created, &claim, Outcome::Changed)?;
    let running = next(Some(&created), &claim)?;
    let reclaim_token = LeaseToken::new(b"reclaim-token".to_vec())?;
    let reclaim = command(
        &running,
        20,
        Operation::Reclaim(Claim {
            expected_state: b"running".to_vec(),
            worker: b"r".to_vec(),
            token: reclaim_token.clone(),
            lease_ms: 10,
        }),
    );
    assert_preserved(&running, &reclaim, Outcome::Changed)?;
    let reclaimed = next(Some(&running), &reclaim)?;
    let retry = command(
        &reclaimed,
        21,
        Operation::Retry(Retry {
            guard: LeaseGuard {
                token: reclaim_token,
                fence: Fence::new(2),
            },
            run_at: RetryTime::Explicit(TimestampMillis::new(25)),
            payload: None,
            error: None,
        }),
    );
    assert_preserved(&reclaimed, &retry, Outcome::Changed)?;
    let scheduled = next(Some(&reclaimed), &retry)?;
    assert_eq!(scheduled.attempts(), 1);
    assert_eq!(scheduled.state(), b"queued");
    assert_preserved(&scheduled, &retry, Outcome::LegacyNoop)?;
    Ok(())
}
