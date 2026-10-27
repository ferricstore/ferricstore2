//! Pinned one-partition bulk observations through the real private byte parent.
//! Physical value references, info queries and original setup remain uncredited.
//! UID suffixes and unspecified creation clocks use deterministic fixture values.

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, ScopedRecordId};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{InstallCommand, PolicyLimits, prepare_install};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{BatchRejection, GroupView};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::indexed::{IndexRejection, IndexRows, IndexedDecision, IndexedGroupView};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::{
    CommittedCommand, Create, DomainError, Operation, Terminal, WorkflowRecord, reduce,
};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecLimits, RecordBatchApplyDecision, RecordBatchApplyPlan, decide_record_batch,
    encode_record_batch,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

use super::TestResult;

fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 1, AppliedPosition::new(index))
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"source-tenant".to_vec())?,
        GroupId::new(b"source-group".to_vec())?,
        RecoveryId::new(b"source-recovery".to_vec())?,
    ))
}

fn claimed(id: &[u8], workflow_type: &[u8], worker: &[u8]) -> TestResult<WorkflowRecord> {
    let create = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(900),
        operation: Operation::Create(Create {
            workflow_type: workflow_type.to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_000)),
            payload: None,
        }),
    };
    let created = reduce(None, &create)?
        .into_parts()
        .1
        .ok_or("create absent")?
        .record;
    let claim = CommittedCommand {
        id: created.id().clone(),
        now: TimestampMillis::new(1_000),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: worker.to_vec(),
            token: LeaseToken::new(id.to_vec())?,
            lease_ms: 30_000,
        }),
    };
    Ok(reduce(Some(&created), &claim)?
        .into_parts()
        .1
        .ok_or("claim absent")?
        .record)
}

fn recipe(complete: bool, stale: bool) -> [&'static [u8]; 5] {
    match (complete, stale) {
        (false, false) => [
            b"tenant-fail-many",
            b"bulk-fail-many",
            b"fail-many-a",
            b"fail-many-b",
            b"worker-fail",
        ],
        (false, true) => [
            b"tenant-fail-many-rollback",
            b"bulk-fail-many-rollback",
            b"fail-many-good",
            b"fail-many-bad",
            b"worker-fail",
        ],
        (true, false) => [
            b"tenant-complete-many",
            b"bulk-complete-many",
            b"complete-many-a",
            b"complete-many-b",
            b"worker-complete",
        ],
        (true, true) => [
            b"tenant-complete-many-rollback",
            b"bulk-complete-many-rollback",
            b"complete-many-good",
            b"complete-many-bad",
            b"worker-complete",
        ],
    }
}

fn mutations(
    records: &[WorkflowRecord],
    partition: &PartitionId,
    complete: bool,
    stale: bool,
) -> TestResult<Vec<PreparedBatchItem>> {
    records
        .iter()
        .map(|record| {
            let bad = stale && record.id().as_bytes().ends_with(b"-bad");
            let terminal = Terminal {
                guard: LeaseGuard {
                    token: record.lease().ok_or("fixture lease absent")?.token.clone(),
                    fence: Fence::new(record.fence().get() + u64::from(bad)),
                },
                payload: None,
                value: (!stale).then(|| {
                    if complete {
                        b"result-batch".to_vec()
                    } else {
                        b"error-batch".to_vec()
                    }
                }),
            };
            Ok(PreparedBatchItem {
                partition: partition.clone(),
                command: CommittedCommand {
                    id: record.id().clone(),
                    now: TimestampMillis::new(2_000),
                    operation: if complete {
                        Operation::Complete(terminal)
                    } else {
                        Operation::Fail(terminal)
                    },
                },
                create_priority: 0,
            })
        })
        .collect()
}

fn source_case(
    complete: bool,
    stale: bool,
    reverse: bool,
    check: impl FnOnce(&RecordBatchApplyPlan<'_>, &[WorkflowRecord], &[Vec<u8>]) -> TestResult,
) -> TestResult {
    let [partition, workflow_type, mut ids @ .., worker] = recipe(complete, stale);
    ids.sort_unstable();
    let partition = PartitionId::new(partition.to_vec())?;
    let records = ids
        .iter()
        .map(|id| claimed(id, workflow_type, worker))
        .collect::<TestResult<Vec<_>>>()?;
    let original = records.clone();
    let entries: Vec<_> = records
        .iter()
        .zip([1_u128, 2])
        .map(|(record, state_enter_seq)| ClaimRecord {
            record,
            partition_key: partition.as_bytes(),
            priority: 0,
            state_enter_seq,
        })
        .collect();
    let current = BTreeMap::from_iter(
        entries
            .iter()
            .map(|entry| (ScopedRecordId::for_record(entry), entry.record)),
    );
    let indexed = BTreeMap::from_iter(
        entries
            .iter()
            .map(|entry| (ScopedRecordId::for_record(entry), *entry)),
    );
    let parallel = BTreeMap::from_iter(
        entries
            .iter()
            .filter_map(|entry| DueOrder::for_record(entry).map(|due| (due, *entry))),
    );
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let scope = scope()?;
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let stamp = clock(1_000)?;
    let control = ControlState::restored(
        stamp,
        EntryIdentity::new(log(0), PreparedDigest::new([0; 32])),
        EntryStamp::Stamped(stamp),
    )?;
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control,
            outcomes: &outcomes,
        },
        group: IndexedGroupView {
            batch: GroupView {
                group: scope.group(),
                placements: &placements,
                records: &current,
            },
            records: &indexed,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
    };
    let mut items = mutations(&records, &partition, complete, stale)?;
    if reverse {
        items.reverse();
    }
    let expected_ids = items
        .iter()
        .map(|item| item.command.id.as_bytes().to_vec())
        .collect::<Vec<_>>();
    let request = PreparedRequest {
        scope: scope.clone(),
        request: RequestId::new(b"source-request".to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(2_000)?,
        operation: PreparedOperation::Batch(items),
    };
    let limits = CodecLimits::new(AdmissionLimits::default());
    let encoded = encode_record_batch(request, limits, 64 * 1_024 * 1_024)?;
    let plan = decide_record_batch(
        encoded.as_bytes(),
        log(1),
        view,
        &retries,
        60_000,
        limits,
        64 * 1_024 * 1_024,
    )?;
    drop(encoded);
    check(&plan, &records, &expected_ids)?;
    assert_eq!(records, original);
    Ok(())
}

#[test]
fn pinned_fail_many_preserves_order_and_atomic_guard_rollback() -> TestResult {
    // Supplemental bridge witness: creation priority and current FIFO mode survive dispatch.
    assert_creation_priority()?;
    // Pinned flow_fail_many_atomically_fails_one_partition_batch.exs:8,52.
    source_case(false, false, false, |plan, _, expected_ids| {
        let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
            return Err("fresh fail result absent".into());
        };
        let IndexedDecision::Accepted(batch) = operation else {
            return Err("valid fail batch rejected".into());
        };
        let writes = batch
            .batch()
            .items()
            .iter()
            .map(|item| item.plan().write().ok_or("write absent"))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            writes
                .iter()
                .map(|write| write.record.id().as_bytes())
                .collect::<Vec<_>>(),
            expected_ids.iter().map(Vec::as_slice).collect::<Vec<_>>()
        );
        assert_eq!(
            writes
                .iter()
                .map(|write| write.record.state())
                .collect::<Vec<_>>(),
            [b"failed".as_slice(); 2]
        );
        assert!(
            writes
                .iter()
                .all(|write| write.record.error() == Some(b"error-batch".as_slice()))
        );
        assert_eq!(
            batch
                .batch()
                .items()
                .iter()
                .filter(|item| item.plan().history().is_some())
                .count(),
            2
        );
        let deltas = batch.deltas().collect::<Vec<_>>();
        assert_eq!(deltas.len(), 2);
        assert!(
            deltas
                .iter()
                .all(|delta| delta.before.is_some() && delta.after.is_none())
        );
        assert_eq!(outcome.response(), [0]);
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        Ok(())
    })?;
    // The pinned recipe sorts bad before good; reversal additionally exercises late refusal.
    for reverse in [false, true] {
        source_case(false, true, reverse, |plan, records, _| {
            let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
                return Err("fresh guard rejection absent".into());
            };
            let reason = IndexRejection::Batch(BatchRejection::Domain(DomainError::StaleLease));
            assert_eq!(operation, &IndexedDecision::Rejected(reason));
            assert_eq!(outcome.status(), RetainedStatus::IndexedRejected(reason));
            assert_eq!(outcome.response(), [1, 0, 4]);
            assert_eq!(
                records
                    .iter()
                    .map(WorkflowRecord::state)
                    .collect::<Vec<_>>(),
                [b"running".as_slice(); 2]
            );
            assert_eq!(
                records
                    .iter()
                    .map(WorkflowRecord::version)
                    .collect::<Vec<_>>(),
                [2, 2]
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn pinned_complete_many_preserves_order_and_atomic_guard_rollback() -> TestResult {
    // Pinned flow_claim_due_automatically_reclaims_expired_leases_ratio.exs:758,807.
    source_case(true, false, false, |plan, _, expected_ids| {
        let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
            return Err("fresh complete result absent".into());
        };
        let IndexedDecision::Accepted(batch) = operation else {
            return Err("valid complete batch rejected".into());
        };
        let writes = batch
            .batch()
            .items()
            .iter()
            .map(|item| item.plan().write().ok_or("write absent"))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            writes
                .iter()
                .map(|write| write.record.id().as_bytes())
                .collect::<Vec<_>>(),
            expected_ids.iter().map(Vec::as_slice).collect::<Vec<_>>()
        );
        assert_eq!(
            writes
                .iter()
                .map(|write| write.record.state())
                .collect::<Vec<_>>(),
            [b"completed".as_slice(); 2]
        );
        assert!(
            writes
                .iter()
                .all(|write| write.record.result() == Some(b"result-batch".as_slice()))
        );
        assert_eq!(
            batch
                .batch()
                .items()
                .iter()
                .filter(|item| item.plan().history().is_some())
                .count(),
            2
        );
        let deltas = batch.deltas().collect::<Vec<_>>();
        assert_eq!(deltas.len(), 2);
        assert!(
            deltas
                .iter()
                .all(|delta| delta.before.is_some() && delta.after.is_none())
        );
        assert_eq!(outcome.response(), [0]);
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        Ok(())
    })?;
    for reverse in [false, true] {
        source_case(true, true, reverse, |plan, records, _| {
            let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
                return Err("fresh guard rejection absent".into());
            };
            let reason = IndexRejection::Batch(BatchRejection::Domain(DomainError::StaleLease));
            assert_eq!(operation, &IndexedDecision::Rejected(reason));
            assert_eq!(outcome.status(), RetainedStatus::IndexedRejected(reason));
            assert_eq!(outcome.response(), [1, 0, 4]);
            assert_eq!(
                records
                    .iter()
                    .map(WorkflowRecord::state)
                    .collect::<Vec<_>>(),
                [b"running".as_slice(); 2]
            );
            assert_eq!(
                records
                    .iter()
                    .map(WorkflowRecord::version)
                    .collect::<Vec<_>>(),
                [2, 2]
            );
            Ok(())
        })?;
    }
    Ok(())
}

fn creation_mode(
    fifo: bool,
    priority: u8,
    check: impl FnOnce(&RecordBatchApplyPlan<'_>) -> TestResult,
) -> TestResult {
    let scope = scope()?;
    let partition = PartitionId::new(b"priority-partition".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let candidate = prepare_install(
        None,
        0,
        &InstallCommand::low_level(
            b"priority-job".to_vec(),
            PolicyPatch {
                states: BTreeMap::from([(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Fifo),
                        ..StatePolicyPatch::default()
                    }),
                )]),
                ..PolicyPatch::default()
            },
        ),
        PolicyLimits::default(),
    )?;
    let policies = if fifo {
        BTreeMap::from([(b"priority-job".as_slice(), candidate.policy())])
    } else {
        BTreeMap::new()
    };
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let lanes = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control: ControlState::initial(clock(0)?),
            outcomes: &outcomes,
        },
        group: IndexedGroupView {
            batch: GroupView {
                group: scope.group(),
                placements: &placements,
                records: &current,
            },
            records: &records,
            parallel: &parallel,
            fifo: &lanes,
            policies: &policies,
        },
    };
    let request = PreparedRequest {
        scope: scope.clone(),
        request: RequestId::new(b"priority-request".to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(1)?,
        operation: PreparedOperation::Batch(vec![PreparedBatchItem {
            partition,
            command: CommittedCommand {
                id: RecordId::new(b"priority-record".to_vec())?,
                now: TimestampMillis::new(1),
                operation: Operation::Create(Create {
                    workflow_type: b"priority-job".to_vec(),
                    state: b"queued".to_vec(),
                    run_at: None,
                    payload: None,
                }),
            },
            create_priority: priority,
        }]),
    };
    let limits = CodecLimits::default();
    let bytes = encode_record_batch(request, limits, 64 * 1_024 * 1_024)?;
    let plan = decide_record_batch(
        bytes.as_bytes(),
        log(0),
        view,
        &retries,
        100,
        limits,
        64 * 1_024 * 1_024,
    )?;
    drop(bytes);
    check(&plan)
}

fn assert_creation_priority() -> TestResult {
    for (fifo, priority) in [(false, 2), (true, 0), (true, 2)] {
        creation_mode(fifo, priority, |plan| {
            let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
                return Err("fresh creation absent".into());
            };
            if fifo && priority != 0 {
                assert_eq!(
                    operation,
                    &IndexedDecision::Rejected(IndexRejection::FifoPriority)
                );
                assert_eq!(
                    outcome.status(),
                    RetainedStatus::IndexedRejected(IndexRejection::FifoPriority)
                );
                assert_eq!(outcome.response(), [1, 5, 0]);
                return Ok(());
            }
            let IndexedDecision::Accepted(batch) = operation else {
                return Err("valid creation rejected".into());
            };
            assert_eq!(
                batch.metadata().next().ok_or("metadata absent")?.priority,
                priority
            );
            assert_eq!(batch.consumed_ordinals(), 1);
            let delta = batch.deltas().next().ok_or("index delta absent")?;
            assert_eq!(delta.before, None);
            if fifo {
                assert!(
                    matches!(delta.after, Some(IndexRows::Fifo { head_candidate: Some(due), .. }) if due.priority == 0)
                );
            } else {
                assert!(
                    matches!(delta.after, Some(IndexRows::Parallel { due }) if due.priority == 2)
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}
