//! Bounded correctness smoke and explicit release measurements for indexed batches.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::hint::black_box;
use std::time::{Duration, Instant};

use wf_domain::claim::DueOrder;
use wf_domain::history::EventKind;
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{BatchItem, BatchRejection, GroupView};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedDecision, IndexedGroupView,
    decide_indexed_batch,
};
use wf_domain::workflow::{CommittedCommand, Create, Operation, Outcome, WorkflowStatus};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let measure = match (arguments.next(), arguments.next()) {
        (None, None) => false,
        (Some(flag), None) if flag == "--test" || flag == "--bench" => false,
        (Some(flag), None) if flag == "--measure" => {
            if cfg!(debug_assertions) {
                return Err("measurement requires the recorded optimized bench build".into());
            }
            true
        }
        _ => return Err("usage: indexed_singleton [--test | --bench | --measure]".into()),
    };
    if measure {
        println!("indexed_singleton: measurement requires recorded bench-profile provenance");
    }
    for count in [1, 2, 32, 1_000] {
        run_case(count, false, measure)?;
    }
    run_case(1, true, measure)?;
    if !measure {
        println!("indexed_singleton smoke: five profiles validated twice");
    }
    Ok(())
}

fn make_commands(count: usize) -> Result<Vec<CommittedCommand>, Box<dyn Error>> {
    if !matches!(count, 1 | 2 | 32 | 1_000) {
        return Err("unsupported bounded benchmark batch size".into());
    }
    (0..count)
        .map(|index| {
            let id = format!("bench-{index:04}").into_bytes();
            Ok(CommittedCommand {
                id: RecordId::new(id.clone())?,
                now: TimestampMillis::new(1_000),
                operation: Operation::Create(Create {
                    workflow_type: b"job".to_vec(),
                    state: b"queued".to_vec(),
                    run_at: Some(TimestampMillis::new(2_000)),
                    payload: Some(id),
                }),
            })
        })
        .collect()
}

fn with_empty_case(
    count: usize,
    run: impl for<'view> FnOnce(
        IndexedGroupView<'view>,
        &'view [IndexedBatchItem<'view>],
    ) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let commands = make_commands(count)?;
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::new();
    let current = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let items: Vec<_> = commands
        .iter()
        .map(|command| {
            IndexedBatchItem::new(BatchItem::new(&partition, command, command.id.as_bytes()))
        })
        .collect();
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
    run(view, &items)
}

fn validate_decision(
    decision: &IndexedDecision<'_>,
    items: &[IndexedBatchItem<'_>],
    refuse: bool,
) -> Result<(), Box<dyn Error>> {
    if refuse {
        assert_eq!(
            decision,
            &IndexedDecision::Rejected(IndexRejection::Batch(BatchRejection::Admission(
                AdmissionError::DecisionBytes,
            )))
        );
        return Ok(());
    }
    let IndexedDecision::Accepted(plan) = decision else {
        return Err("accepted benchmark profile was rejected".into());
    };
    let count = u32::try_from(items.len())?;
    assert_eq!(plan.batch().items().len(), items.len());
    assert_eq!(plan.metadata().len(), items.len());
    assert_eq!(plan.deltas().len(), items.len());
    assert_eq!(plan.consumed_ordinals(), count);
    assert_eq!(plan.next_ordinal(), Some(count));
    for (ordinal, (((scoped, item), metadata), delta)) in plan
        .batch()
        .items()
        .iter()
        .zip(items)
        .zip(plan.metadata())
        .zip(plan.deltas())
        .enumerate()
    {
        assert_eq!(scoped.partition().as_bytes(), b"p");
        assert_eq!(scoped.plan().outcome(), Outcome::Changed);
        let write = scoped.plan().write().ok_or("missing created write")?;
        let event = scoped.plan().history().ok_or("missing created history")?;
        let record = &write.record;
        assert_eq!(write.expected_version, None);
        validate_created_record(record, item);
        assert_eq!(event.event_id, "1000-1");
        assert_eq!(event.kind, EventKind::Created);
        assert_eq!(event.occurred_at, TimestampMillis::new(1_000));
        assert_eq!(
            (event.version, event.fence.get(), event.attempts),
            (1, 0, 0)
        );
        assert_eq!(event.state, b"queued");
        assert_eq!(event.next_run_at, Some(TimestampMillis::new(2_000)));
        assert_eq!(event.lease_deadline, TimestampMillis::new(0));
        assert!(event.worker.is_none() && event.retry.is_none());
        assert_eq!(metadata.priority, 0);
        assert_eq!(
            metadata.state_enter_seq,
            (1_u128 << 32) + u128::from(u32::try_from(ordinal)?)
        );
        assert_eq!(delta.before, None);
        assert_eq!(
            delta.after,
            Some(IndexRows::Parallel {
                due: DueOrder {
                    priority: 0,
                    due: TimestampMillis::new(2_000),
                    id: item.item.command.id.as_bytes(),
                    partition_key: b"p",
                    state: b"queued",
                    workflow_type: b"job",
                },
            })
        );
    }
    Ok(())
}

fn sample_decisions(
    view: IndexedGroupView<'_>,
    items: &[IndexedBatchItem<'_>],
    limits: AdmissionLimits,
    iterations: usize,
) -> Result<Duration, Box<dyn Error>> {
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(1)),
        first_ordinal: 0,
    };
    let refuse = limits.decision_bytes() == 1_023;
    let started = Instant::now();
    for _ in 0..iterations {
        let decision = decide_indexed_batch(
            black_box(view),
            black_box(items),
            black_box(order),
            black_box(b"indexed-bench".as_slice()),
            black_box(limits),
        )?;
        let expected = match &decision {
            IndexedDecision::Accepted(_) => !refuse,
            IndexedDecision::Rejected(error) => {
                refuse
                    && *error
                        == IndexRejection::Batch(BatchRejection::Admission(
                            AdmissionError::DecisionBytes,
                        ))
            }
        };
        if !expected {
            return Err("unexpected timed decision; discard this measurement".into());
        }
        drop(black_box(decision));
    }
    Ok(started.elapsed())
}

fn run_case(count: usize, refuse: bool, measure: bool) -> Result<(), Box<dyn Error>> {
    if refuse && count != 1 {
        return Err("the refusal control must be a singleton".into());
    }
    let defaults = AdmissionLimits::default();
    let limits = if refuse {
        AdmissionLimits::new(defaults.inline_bytes(), 1_023, defaults.batch_items())?
    } else {
        defaults
    };
    with_empty_case(count, |view, items| {
        let order = EntryOrder {
            identity: LogIdentity::new(1, 7, AppliedPosition::new(1)),
            first_ordinal: 0,
        };
        let before = decide_indexed_batch(view, items, order, b"indexed-bench", limits)?;
        validate_decision(&before, items, refuse)?;
        drop(before);
        if measure {
            let iterations = (4_096 / count).max(1);
            let _warmup = sample_decisions(view, items, limits, iterations)?;
            for sample in 0..16 {
                let elapsed = sample_decisions(view, items, limits, iterations)?;
                println!(
                    "count={count} refusal={refuse} sample={sample} iterations={iterations} elapsed_ns={}",
                    elapsed.as_nanos()
                );
            }
        }
        let after = decide_indexed_batch(view, items, order, b"indexed-bench", limits)?;
        validate_decision(&after, items, refuse)
    })
}

fn validate_created_record(
    record: &wf_domain::workflow::WorkflowRecord,
    item: &IndexedBatchItem<'_>,
) {
    assert_eq!(record.id(), &item.item.command.id);
    assert_eq!(record.workflow_type(), b"job");
    assert_eq!(record.state(), b"queued");
    assert_eq!(record.payload(), Some(item.item.command.id.as_bytes()));
    assert_eq!(
        (record.version(), record.fence().get(), record.attempts()),
        (1, 0, 0)
    );
    assert_eq!(record.created_at(), TimestampMillis::new(1_000));
    assert_eq!(record.updated_at(), TimestampMillis::new(1_000));
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(2_000)));
    assert_eq!(record.status(), WorkflowStatus::Scheduled);
    assert!(record.lease().is_none());
}
