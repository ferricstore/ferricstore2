//! Consumed FIFO heads are canonical and cannot occupy both correctness indexes.
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
    EntryOrder, IndexedBatchItem, IndexedGroupView, decide_indexed_batch,
};
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn command(id: &[u8], operation: Operation) -> Result<CommittedCommand, TestError> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(10),
        operation,
    })
}
fn record(id: &[u8]) -> Result<WorkflowRecord, TestError> {
    Ok(reduce(
        None,
        &command(
            id,
            Operation::Create(Create {
                workflow_type: b"job".to_vec(),
                state: b"queued".to_vec(),
                run_at: None,
                payload: None,
            }),
        )?,
    )?
    .into_parts()
    .1
    .ok_or("required test fixture value absent")?
    .record)
}
fn claim(id: &[u8]) -> Result<CommittedCommand, TestError> {
    command(
        id,
        Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"token".to_vec())?,
            lease_ms: 100,
        }),
    )
}
fn with_view(test: impl FnOnce(IndexedGroupView<'_>, &PartitionId) -> TestResult) -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let a = record(b"a")?;
    let b = record(b"b")?;
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
    let fifo = BTreeMap::from([(
        LaneKey::for_record(&entries[0]).ok_or("required test fixture value absent")?,
        BTreeMap::from(entries.map(|entry| (LaneOrder::for_record(&entry), entry))),
    )]);
    let parallel = BTreeMap::new();
    let patch = PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    let policy = prepare_install(
        None,
        0,
        &InstallCommand::low_level(b"job".to_vec(), patch),
        PolicyLimits::default(),
    )?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
    test(
        IndexedGroupView {
            batch: GroupView {
                group: &group,
                placements: &placements,
                records: &records,
            },
            records: &current,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
        &partition,
    )
}
fn order() -> EntryOrder {
    EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
        first_ordinal: 0,
    }
}

#[test]
fn consumed_fifo_head_must_be_bound_to_both_canonical_record_views() -> TestResult {
    with_view(|view, partition| {
        let follower = claim(b"b")?;
        let items = [IndexedBatchItem::new(BatchItem::new(
            partition, &follower, b"key",
        ))];
        let follower_id = ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: b"b",
        };
        let incomplete = BTreeMap::from([(follower_id, view.records[&follower_id].record)]);
        assert_eq!(
            decide_indexed_batch(
                IndexedGroupView {
                    batch: GroupView {
                        records: &incomplete,
                        ..view.batch
                    },
                    ..view
                },
                &items,
                order(),
                b"entry",
                AdmissionLimits::default()
            ),
            Err(ApplyError::InvalidAuthority)
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn consumed_fifo_row_cannot_also_occupy_parallel_due_authority() -> TestResult {
    with_view(|view, partition| {
        let head_id = ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: b"a",
        };
        let entry = view.records[&head_id];
        let duplicate = BTreeMap::from([(
            DueOrder::for_record(&entry).ok_or("required test fixture value absent")?,
            entry,
        )]);
        let head = claim(b"a")?;
        let items = [IndexedBatchItem::new(BatchItem::new(
            partition, &head, b"key",
        ))];
        assert_eq!(
            decide_indexed_batch(
                IndexedGroupView {
                    parallel: &duplicate,
                    ..view
                },
                &items,
                order(),
                b"entry",
                AdmissionLimits::default()
            ),
            Err(ApplyError::InvalidAuthority)
        );
        Ok(())
    })?;
    Ok(())
}
