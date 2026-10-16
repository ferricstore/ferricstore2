//! Pinned independent-transition components; physical/profile parity is separate.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::ScopedRecordId;
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{
    ApplyError, BatchDecision, BatchItem, BatchRejection, GroupView, IndependentTransitionDecision,
    IndependentTransitionPlan, ScopedPlan, decide_group_batch, decide_independent_transition_batch,
};
use wf_domain::workflow::{
    CommittedCommand, Create, DomainError, Operation, Outcome, RecordWrite, Transition,
    WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn initial(id: &[u8], kind: &[u8]) -> TestResult<WorkflowRecord> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: kind.to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_000)),
            payload: None,
        }),
    };
    Ok(reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("create missing")?
        .record)
}

fn transition(
    id: &[u8],
    from: &[u8],
    to: &[u8],
    fence: u64,
    now: u64,
    payload: Option<Vec<u8>>,
) -> TestResult<CommittedCommand> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(now),
        operation: Operation::Transition(Transition {
            from: from.to_vec(),
            to: to.to_vec(),
            fence: Fence::new(fence),
            lease_token: None,
            run_at: Some(TimestampMillis::new(now)),
            payload,
        }),
    })
}

fn accepted(decision: IndependentTransitionDecision) -> TestResult<IndependentTransitionPlan> {
    match decision {
        IndependentTransitionDecision::Accepted(plan) => Ok(plan),
        IndependentTransitionDecision::Rejected(error) => {
            Err(format!("unexpected whole rejection: {error:?}").into())
        }
    }
}

fn success(results: &[Result<ScopedPlan, DomainError>], index: usize) -> TestResult<&ScopedPlan> {
    Ok(results
        .get(index)
        .ok_or("result missing")?
        .as_ref()
        .map_err(|error| *error)?)
}

fn write(plan: &ScopedPlan) -> TestResult<&RecordWrite> {
    Ok(plan.plan().write().ok_or("write missing")?)
}

fn decide(
    rows: &[(&PartitionId, &WorkflowRecord)],
    commands: &[(&PartitionId, &CommittedCommand)],
    entry: &[u8],
    limits: AdmissionLimits,
) -> TestResult<IndependentTransitionDecision> {
    let group = GroupId::new(b"g".to_vec())?;
    let placements = commands
        .iter()
        .map(|(partition, _)| ((*partition).clone(), group.clone()))
        .collect();
    let records = rows
        .iter()
        .map(|(partition, row)| {
            (
                ScopedRecordId {
                    partition_key: partition.as_bytes(),
                    id: row.id().as_bytes(),
                },
                *row,
            )
        })
        .collect();
    let items: Vec<_> = commands
        .iter()
        .map(|(partition, command)| BatchItem::new(partition, command, b"k"))
        .collect();
    Ok(decide_independent_transition_batch(
        GroupView {
            group: &group,
            placements: &placements,
            records: &records,
        },
        &items,
        entry,
        limits,
    )?)
}

#[test]
fn independent_transition_many_keeps_success_after_stale_guard() -> TestResult {
    // Source 67e5682f41dbbb5b:545..574. uid(prefix) uses a positive :suffix.
    let partition = PartitionId::new(b"tenant-transition-many-independent:1".to_vec())?;
    let kind = b"bulk-transition-independent:1";
    let bad = initial(b"transition-independent-bad:1", kind)?;
    let good = initial(b"transition-independent-good:1", kind)?;
    let commands = [
        transition(bad.id().as_bytes(), b"queued", b"ready", 1, 2_000, None)?,
        transition(good.id().as_bytes(), b"queued", b"ready", 0, 2_000, None)?,
    ];
    let plan = accepted(decide(
        &[(&partition, &bad), (&partition, &good)],
        &[(&partition, &commands[0]), (&partition, &commands[1])],
        &[],
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 2);
    assert_eq!(plan.results()[0], Err(DomainError::StaleLease));
    assert_eq!(DomainError::StaleLease.to_string(), "ERR stale flow lease");
    assert_eq!(bad.state(), b"queued");
    assert_eq!(bad.version(), 1);
    let changed = success(plan.results(), 1)?;
    assert_eq!(changed.partition(), &partition);
    let written = write(changed)?;
    assert_eq!(written.expected_version, Some(1));
    assert_eq!(written.record.id(), good.id());
    assert_eq!(written.record.state(), b"ready");
    assert_eq!(written.record.version(), 2);
    assert_eq!(
        written.record.next_run_at(),
        Some(TimestampMillis::new(2_000))
    );
    Ok(())
}

#[test]
fn independent_transition_pipeline_preserves_duplicate_order() -> TestResult {
    // Source 5c2bdf9abea86479:577..599; only unasserted clock values are fixed.
    let partition = PartitionId::new(b"tenant-transition-pipeline-duplicate:1".to_vec())?;
    let record = initial(b"transition-pipeline-duplicate:1", b"pipeline-duplicate:1")?;
    assert_eq!(record.state(), b"queued");
    let commands = [
        transition(record.id().as_bytes(), b"queued", b"ready", 0, 2_000, None)?,
        transition(
            record.id().as_bytes(),
            b"ready",
            b"processing",
            0,
            3_000,
            None,
        )?,
    ];
    let plan = accepted(decide(
        &[(&partition, &record)],
        &[(&partition, &commands[0]), (&partition, &commands[1])],
        &[],
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 2);
    let first = write(success(plan.results(), 0)?)?;
    let last = write(success(plan.results(), 1)?)?;
    assert_eq!(first.expected_version, Some(1));
    assert_eq!(first.record.state(), b"ready");
    assert_eq!(first.record.version(), 2);
    assert_eq!(last.expected_version, Some(2));
    assert_eq!(last.record.state(), b"processing");
    assert_eq!(last.record.version(), 3);
    assert_eq!(last.record.id(), record.id());
    assert_atomic_duplicate_unchanged(&partition, &record, &commands)?;
    Ok(())
}

fn assert_atomic_duplicate_unchanged(
    partition: &PartitionId,
    record: &WorkflowRecord,
    commands: &[CommittedCommand; 2],
) -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::from([(
        ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: record.id().as_bytes(),
        },
        record,
    )]);
    let items = commands
        .each_ref()
        .map(|command| BatchItem::new(partition, command, b"k"));
    assert_eq!(
        decide_group_batch(
            GroupView {
                group: &group,
                placements: &placements,
                records: &records
            },
            &items,
            &[],
            AdmissionLimits::default(),
        )?,
        BatchDecision::Rejected(BatchRejection::DuplicateId)
    );
    Ok(())
}

#[test]
fn independent_transition_failure_and_noop_keep_latest_success() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = initial(b"i", b"r")?;
    let commands = [
        transition(b"i", b"queued", b"ready", 0, 2_000, None)?,
        transition(b"i", b"queued", b"processing", 0, 2_001, None)?,
        transition(b"i", b"queued", b"ready", 0, 2_002, None)?,
        transition(b"i", b"ready", b"processing", 0, 3_000, None)?,
    ];
    let refs: Vec<_> = commands
        .iter()
        .map(|command| (&partition, command))
        .collect();
    let plan = accepted(decide(
        &[(&partition, &record)],
        &refs,
        &[],
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 4);
    assert_eq!(plan.results()[1], Err(DomainError::WrongState));
    let noop = success(plan.results(), 2)?.plan();
    assert_eq!(noop.outcome(), Outcome::LegacyNoop);
    assert!(noop.write().is_none());
    assert!(noop.history().is_none());
    assert_eq!(
        plan.results()
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .filter(|result| result.plan().write().is_some())
            .count(),
        2
    );
    assert_eq!(
        plan.results()
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .filter(|result| result.plan().history().is_some())
            .count(),
        2
    );
    let last = write(success(plan.results(), 3)?)?;
    assert_eq!(last.expected_version, Some(2));
    assert_eq!(last.record.state(), b"processing");
    assert_eq!(last.record.version(), 3);
    assert_partitions_are_independent()?;
    Ok(())
}

fn assert_partitions_are_independent() -> TestResult {
    let first = PartitionId::new(b"p".to_vec())?;
    let second = PartitionId::new(b"q".to_vec())?;
    let record = initial(b"i", b"r")?;
    let commands = [
        transition(b"i", b"queued", b"ready", 0, 2_000, None)?,
        transition(b"i", b"queued", b"processing", 0, 2_000, None)?,
    ];
    let plan = accepted(decide(
        &[(&first, &record), (&second, &record)],
        &[(&first, &commands[0]), (&second, &commands[1])],
        &[],
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 2);
    for (index, partition, state) in [
        (0, &first, b"ready".as_slice()),
        (1, &second, b"processing".as_slice()),
    ] {
        let result = success(plan.results(), index)?;
        assert_eq!(result.partition(), partition);
        let written = write(result)?;
        assert_eq!(written.expected_version, Some(1));
        assert_eq!(written.record.state(), state);
        assert_eq!(written.record.version(), 2);
    }
    Ok(())
}

fn owned_pipeline(cap: usize, with_payload: bool) -> TestResult<IndependentTransitionDecision> {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = initial(b"i", b"r")?;
    let commands = [
        transition(
            b"i",
            b"queued",
            b"ready",
            0,
            2_000,
            with_payload.then(|| vec![7; 256]),
        )?,
        transition(b"i", b"ready", b"processing", 0, 3_000, None)?,
    ];
    let encoded = Vec::new();
    // All current, command, query-map and encoded backing drops at this return.
    decide(
        &[(&partition, &record)],
        &[(&partition, &commands[0]), (&partition, &commands[1])],
        &encoded,
        AdmissionLimits::new(256, cap, 2)?,
    )
}

#[test]
fn independent_transition_limits_and_ownership_are_cumulative() -> TestResult {
    // BEFORE SUT: V=14+8+18+7=47, fixed1152*2=2304 ->2492.
    // First-only payload256: V=270+8+18+263=559 ->4540.
    for (cap, payload, accepts) in [
        (2_492, false, true),
        (2_491, false, false),
        (4_540, true, true),
        (4_539, true, false),
    ] {
        let decision = owned_pipeline(cap, payload)?;
        if accepts {
            let results = accepted(decision)?.into_results();
            assert_eq!(results.len(), 2);
            for index in 0..2 {
                let written = write(success(&results, index)?)?;
                assert_eq!(
                    written.record.payload(),
                    payload.then_some([7; 256].as_slice())
                );
                assert_eq!(written.record.version(), u64::try_from(index)? + 2);
            }
            assert_eq!(write(success(&results, 1)?)?.record.state(), b"processing");
        } else {
            assert_eq!(
                decision,
                IndependentTransitionDecision::Rejected(BatchRejection::Admission(
                    AdmissionError::DecisionBytes
                ))
            );
        }
    }
    assert_limits_and_empty()?;
    assert_authority_and_operation()?;
    Ok(())
}

fn assert_limits_and_empty() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = initial(b"i", b"r")?;
    let first = transition(b"i", b"queued", b"ready", 0, 2_000, Some(vec![7; 256]))?;
    let second = transition(b"i", b"ready", b"processing", 0, 3_000, None)?;
    let rows = [(&partition, &record)];
    let commands = [(&partition, &first), (&partition, &second)];
    assert_eq!(
        decide(&rows, &commands, &[], AdmissionLimits::new(256, 10_000, 1)?)?,
        IndependentTransitionDecision::Rejected(BatchRejection::Admission(
            AdmissionError::BatchItems
        ))
    );
    assert_eq!(
        decide(
            &rows,
            &commands,
            &[0; 5_001],
            AdmissionLimits::new(256, 5_000, 2)?
        )?,
        IndependentTransitionDecision::Rejected(BatchRejection::Admission(
            AdmissionError::DecisionBytes
        ))
    );
    assert_eq!(
        decide(&rows, &commands, &[], AdmissionLimits::new(255, 10_000, 2)?)?,
        IndependentTransitionDecision::Rejected(BatchRejection::Admission(
            AdmissionError::ValueTooLarge {
                bytes: 256,
                maximum: 255
            }
        ))
    );
    let empty = accepted(decide(&[], &[], &[], AdmissionLimits::default())?)?;
    assert_eq!(
        empty.into_results(),
        [] as [Result<ScopedPlan, DomainError>; 0]
    );
    Ok(())
}

fn assert_authority_and_operation() -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let other = GroupId::new(b"other".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let command = transition(b"i", b"queued", b"ready", 0, 2_000, None)?;
    let items = [BatchItem::new(&partition, &command, b"k")];
    let empty = BTreeMap::new();
    let records = BTreeMap::new();
    assert_eq!(
        decide_independent_transition_batch(
            GroupView {
                group: &group,
                placements: &empty,
                records: &records
            },
            &items,
            &[],
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    let placements = BTreeMap::from([(partition.clone(), other)]);
    assert_eq!(
        decide_independent_transition_batch(
            GroupView {
                group: &group,
                placements: &placements,
                records: &records
            },
            &items,
            &[],
            AdmissionLimits::default()
        )?,
        IndependentTransitionDecision::Rejected(BatchRejection::CrossGroup)
    );
    let foreign = initial(b"foreign", b"r")?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::from([(
        ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: b"i",
        },
        &foreign,
    )]);
    assert_eq!(
        decide_independent_transition_batch(
            GroupView {
                group: &group,
                placements: &placements,
                records: &records
            },
            &items,
            &[],
            AdmissionLimits::default()
        ),
        Err(ApplyError::InvalidAuthority)
    );
    let create = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    };
    assert_eq!(
        decide(
            &[],
            &[(&partition, &create)],
            &[],
            AdmissionLimits::default()
        )?,
        IndependentTransitionDecision::Rejected(BatchRejection::Domain(
            DomainError::UnsupportedOperation
        ))
    );
    Ok(())
}
