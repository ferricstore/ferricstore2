//! Canonical metadata, FIFO authority and deterministic committed index deltas.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::lease::LeaseToken;
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{InstallCommand, PolicyLimits, prepare_install};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, BatchItem, GroupView};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedBatchPlan, IndexedDecision,
    IndexedGroupView, decide_indexed_batch,
};
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, Transition, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn command(id: &[u8], now: u64, operation: Operation) -> Result<CommittedCommand, TestError> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(now),
        operation,
    })
}
fn create(id: &[u8]) -> Result<CommittedCommand, TestError> {
    command(
        id,
        10,
        Operation::Create(Create {
            workflow_type: b"job".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    )
}
fn record(command: &CommittedCommand) -> Result<WorkflowRecord, TestError> {
    Ok(reduce(None, command)?
        .into_parts()
        .1
        .ok_or("required test fixture value absent")?
        .record)
}
fn order(index: u64, first_ordinal: u32) -> EntryOrder {
    EntryOrder {
        identity: LogIdentity::new(3, 7, AppliedPosition::new(index)),
        first_ordinal,
    }
}
fn fifo_policy() -> Result<wf_domain::policy::installation::PolicyCandidate, TestError> {
    let patch = PolicyPatch {
        states: BTreeMap::from([
            (
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            ),
            (
                b"next".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            ),
        ]),
        ..PolicyPatch::default()
    };
    Ok(prepare_install(
        None,
        0,
        &InstallCommand::low_level(b"job".to_vec(), patch),
        PolicyLimits::default(),
    )?)
}

fn claim(id: &[u8]) -> Result<CommittedCommand, TestError> {
    command(
        id,
        10,
        Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"token".to_vec())?,
            lease_ms: 100,
        }),
    )
}

fn assert_claim_metadata(plan: &IndexedBatchPlan<'_>) -> TestResult {
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("ordering metadata absent")?
            .state_enter_seq,
        100
    );
    assert_eq!(plan.consumed_ordinals(), 0);
    let delta = plan.deltas().next().ok_or("index delta absent")?;
    let Some(IndexRows::Fifo {
        member,
        head_candidate,
        ..
    }) = delta.after
    else {
        return Err("missing fifo row".into());
    };
    assert_eq!(member.score(), -1);
    assert_eq!(
        head_candidate.ok_or("scheduled FIFO candidate absent")?.due,
        TimestampMillis::new(110)
    );
    Ok(())
}

#[test]
fn creation_uses_log_index_and_bounded_offsets_without_copying_authority() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::new();
    let current = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policy = fifo_policy()?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
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
    let a = create(b"a")?;
    let b = create(b"b")?;
    let items = [
        IndexedBatchItem::new(BatchItem::new(&partition, &a, b"key-a")),
        IndexedBatchItem::new(BatchItem::new(&partition, &b, b"key-b")),
    ];
    let decision = decide_indexed_batch(
        view,
        &items,
        order(20, 4),
        b"entry",
        AdmissionLimits::default(),
    )?;
    let IndexedDecision::Accepted(plan) = &decision else {
        return Err("unexpected rejection".into());
    };
    let metadata: Vec<_> = plan.metadata().collect();
    assert_eq!(metadata[0].state_enter_seq, (20_u128 << 32) + 4);
    assert_eq!(metadata[1].state_enter_seq, (20_u128 << 32) + 5);
    assert_eq!(plan.consumed_ordinals(), 2);
    assert_eq!(plan.next_ordinal(), Some(6));
    let deltas: Vec<_> = plan.deltas().collect();
    assert!(deltas.iter().all(|delta| delta.before.is_none()));
    assert!(
        deltas
            .iter()
            .all(|delta| matches!(delta.after, Some(IndexRows::Fifo { .. })))
    );
    assert_eq!(
        decision,
        decide_indexed_batch(
            view,
            &items,
            order(20, 4),
            b"entry",
            AdmissionLimits::default()
        )?
    );
    assert_eq!(
        decide_indexed_batch(
            view,
            &items,
            order(20, u32::MAX),
            b"entry",
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    let IndexedDecision::Accepted(last) = decide_indexed_batch(
        view,
        &items[..1],
        order(u64::MAX, u32::MAX),
        b"entry",
        AdmissionLimits::default(),
    )?
    else {
        return Err("last ordinal rejected".into());
    };
    assert_eq!(
        last.metadata()
            .next()
            .ok_or("required test fixture value absent")?
            .state_enter_seq,
        (u128::from(u64::MAX) << 32) + u128::from(u32::MAX)
    );
    assert_eq!(last.next_ordinal(), None);
    Ok(())
}

#[test]
fn named_fifo_claim_requires_real_head_and_preserves_logical_sequence() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let a = record(&create(b"a")?)?;
    let b = record(&create(b"b")?)?;
    let entries = [
        ClaimRecord {
            record: &a,
            partition_key: partition.as_bytes(),
            priority: 0,
            state_enter_seq: 100,
        },
        ClaimRecord {
            record: &b,
            partition_key: partition.as_bytes(),
            priority: 0,
            state_enter_seq: 101,
        },
    ];
    let records =
        BTreeMap::from(entries.map(|entry| (ScopedRecordId::for_record(&entry), entry.record)));
    let current = BTreeMap::from(entries.map(|entry| (ScopedRecordId::for_record(&entry), entry)));
    let lane_key = LaneKey::for_record(&entries[0]).ok_or("required test fixture value absent")?;
    let lane = BTreeMap::from(entries.map(|entry| (LaneOrder::for_record(&entry), entry)));
    let fifo = BTreeMap::from([(lane_key, lane)]);
    let parallel = BTreeMap::new();
    let policy = fifo_policy()?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
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
    let follower = claim(b"b")?;
    let items = [IndexedBatchItem::new(BatchItem::new(
        &partition, &follower, b"key",
    ))];
    assert_eq!(
        decide_indexed_batch(
            view,
            &items,
            order(2, 0),
            b"entry",
            AdmissionLimits::default()
        )?,
        IndexedDecision::Rejected(IndexRejection::NotFifoHead)
    );
    let head = claim(b"a")?;
    let items = [IndexedBatchItem::new(BatchItem::new(
        &partition, &head, b"key",
    ))];
    let IndexedDecision::Accepted(plan) = decide_indexed_batch(
        view,
        &items,
        order(2, u32::MAX),
        b"entry",
        AdmissionLimits::default(),
    )?
    else {
        return Err("head rejected".into());
    };
    assert_claim_metadata(&plan)?;
    assert_eq!(a.state(), b"queued");
    let missing_fifo = BTreeMap::new();
    assert_eq!(
        decide_indexed_batch(
            IndexedGroupView {
                fifo: &missing_fifo,
                ..view
            },
            &items,
            order(2, 0),
            b"entry",
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    Ok(())
}

#[test]
fn state_change_reallocates_sequence_and_parallel_priority_is_canonical() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let old = record(&create(b"a")?)?;
    let entry = ClaimRecord {
        record: &old,
        partition_key: partition.as_bytes(),
        priority: 2,
        state_enter_seq: 77,
    };
    let records = BTreeMap::from([(ScopedRecordId::for_record(&entry), &old)]);
    let current = BTreeMap::from([(ScopedRecordId::for_record(&entry), entry)]);
    let parallel = BTreeMap::from([(
        DueOrder::for_record(&entry).ok_or("required test fixture value absent")?,
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
    let transition = command(
        b"a",
        20,
        Operation::Transition(Transition {
            from: b"queued".to_vec(),
            to: b"next".to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: None,
        }),
    )?;
    let mut item = IndexedBatchItem::new(BatchItem::new(&partition, &transition, b"key"));
    item.create_priority = 1;
    let IndexedDecision::Accepted(plan) = decide_indexed_batch(
        view,
        &[item],
        order(3, 2),
        b"entry",
        AdmissionLimits::default(),
    )?
    else {
        return Err("transition rejected".into());
    };
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("required test fixture value absent")?
            .priority,
        2
    );
    assert_eq!(
        plan.metadata()
            .next()
            .ok_or("required test fixture value absent")?
            .state_enter_seq,
        (3_u128 << 32) + 2
    );
    let delta = plan
        .deltas()
        .next()
        .ok_or("required test fixture value absent")?;
    assert!(
        matches!(delta.before, Some(IndexRows::Parallel { due }) if due.priority == 2 && due.state == b"queued")
    );
    assert!(
        matches!(delta.after, Some(IndexRows::Parallel { due }) if due.priority == 2 && due.state == b"next")
    );
    let bad_entry = ClaimRecord {
        state_enter_seq: 78,
        ..entry
    };
    let bad_current = BTreeMap::from([(ScopedRecordId::for_record(&entry), bad_entry)]);
    assert_eq!(
        decide_indexed_batch(
            IndexedGroupView {
                records: &bad_current,
                ..view
            },
            &[item],
            order(3, 2),
            b"entry",
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    Ok(())
}
