//! Pinned pure terminal components; public I/O and runtime profiles stay separate.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::RetryOverride;
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{
    BatchItem, BatchRejection, GroupView, IndependentBatchDecision, IndependentBatchPlan,
    IndependentTransitionDecision, ScopedPlan, decide_independent_terminal_batch,
    decide_independent_transition_batch,
};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, DomainError, Operation, Outcome, RecordWrite, Terminal,
    Transition, WorkflowRecord, reduce,
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

fn claimed(id: &[u8], kind: &[u8], worker: &[u8]) -> TestResult<WorkflowRecord> {
    let initial = initial(id, kind)?;
    let command = CommittedCommand {
        id: initial.id().clone(),
        now: TimestampMillis::new(1_000),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: worker.to_vec(),
            token: LeaseToken::new(b"k".to_vec())?,
            lease_ms: 30_000,
        }),
    };
    // Named claims are real reducer setup, not claim_due selection coverage.
    Ok(reduce(Some(&initial), &command)?
        .into_parts()
        .1
        .ok_or("claim missing")?
        .record)
}

fn guard(record: &WorkflowRecord) -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: record.lease().ok_or("lease missing")?.token.clone(),
        fence: record.fence(),
    })
}

fn complete(
    record: &WorkflowRecord,
    now: u64,
    value: Option<Vec<u8>>,
) -> TestResult<CommittedCommand> {
    Ok(CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(now),
        operation: Operation::Complete(Terminal {
            guard: guard(record)?,
            payload: None,
            value,
        }),
    })
}

fn accepted(decision: IndependentBatchDecision) -> TestResult<IndependentBatchPlan> {
    match decision {
        IndependentBatchDecision::Accepted(plan) => Ok(plan),
        IndependentBatchDecision::Rejected(error) => {
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
    partition: &PartitionId,
    records: &[&WorkflowRecord],
    commands: &[CommittedCommand],
    entry: &[u8],
    limits: AdmissionLimits,
) -> TestResult<IndependentBatchDecision> {
    let group = GroupId::new(b"g".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = records
        .iter()
        .map(|record| {
            (
                ScopedRecordId {
                    partition_key: partition.as_bytes(),
                    id: record.id().as_bytes(),
                },
                *record,
            )
        })
        .collect();
    let items: Vec<_> = commands
        .iter()
        .map(|command| BatchItem::new(partition, command, b"k"))
        .collect();
    Ok(decide_independent_terminal_batch(
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

fn many_record(label: &str, bad: bool, cancel: bool) -> TestResult<WorkflowRecord> {
    let side = if bad { "bad" } else { "good" };
    let id = format!("{label}-independent-{side}:1");
    // Separate source uid calls stay distinct; Cancel shares one type.
    let suffix = if bad || cancel { 1 } else { 2 };
    let kind = format!("{label}-independent:{suffix}");
    if cancel {
        initial(id.as_bytes(), kind.as_bytes())
    } else {
        let worker = format!("worker-{label}-independent");
        claimed(id.as_bytes(), kind.as_bytes(), worker.as_bytes())
    }
}

fn many_command(
    record: &WorkflowRecord,
    kind: EventKind,
    bad: bool,
) -> TestResult<CommittedCommand> {
    let fence = Fence::new(record.fence().get() + u64::from(bad));
    let operation = if kind == EventKind::Cancelled {
        Operation::Cancel(Cancel {
            fence,
            lease_token: None,
            expected_state: None,
            expected_version: None,
            error: None,
        })
    } else {
        let mut guard = guard(record)?;
        guard.fence = fence;
        match kind {
            EventKind::Completed => Operation::Complete(Terminal {
                guard,
                payload: None,
                value: None,
            }),
            EventKind::Failed => Operation::Fail(Terminal {
                guard,
                payload: None,
                value: None,
            }),
            EventKind::Retry => Operation::Retry(Retry {
                guard,
                run_at: RetryTime::Explicit(TimestampMillis::new(3_000)),
                payload: None,
                error: None,
            }),
            _ => return Err("unexpected terminal fixture".into()),
        }
    };
    Ok(CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(2_000),
        operation,
    })
}

#[test]
fn independent_terminal_many_preserves_per_item_success() -> TestResult {
    // Source 4963f0d0a4e54c1e:310..466, all four operation groups.
    let partition = PartitionId::new(b"tenant-terminal-many-independent:1".to_vec())?;
    for (label, kind, state) in [
        ("complete", EventKind::Completed, b"completed".as_slice()),
        ("retry", EventKind::Retry, b"queued".as_slice()),
        ("fail", EventKind::Failed, b"failed".as_slice()),
        ("cancel", EventKind::Cancelled, b"cancelled".as_slice()),
    ] {
        let cancel = kind == EventKind::Cancelled;
        let bad = many_record(label, true, cancel)?;
        let good = many_record(label, false, cancel)?;
        let commands = [
            many_command(&bad, kind, true)?,
            many_command(&good, kind, false)?,
        ];
        let plan = accepted(decide(
            &partition,
            &[&bad, &good],
            &commands,
            &[],
            AdmissionLimits::default(),
        )?)?;
        assert_eq!(plan.results().len(), 2);
        assert_eq!(plan.results()[0], Err(DomainError::StaleLease));
        assert_eq!(DomainError::StaleLease.to_string(), "ERR stale flow lease");
        assert_eq!(
            bad.state(),
            if cancel {
                b"queued".as_slice()
            } else {
                b"running"
            }
        );
        assert_eq!(bad.version(), if cancel { 1 } else { 2 });
        let successful = success(plan.results(), 1)?;
        assert_eq!(successful.partition(), &partition);
        assert_eq!(successful.plan().outcome(), Outcome::Changed);
        let changed = write(successful)?;
        assert_eq!(changed.record.id(), good.id());
        assert_eq!(changed.expected_version, Some(if cancel { 1 } else { 2 }));
        assert_eq!(changed.record.version(), if cancel { 2 } else { 3 });
        assert_eq!(changed.record.state(), state);
        assert!(changed.record.lease().is_none());
        assert_eq!(
            successful.plan().history().ok_or("history missing")?.kind,
            kind
        );
        assert_eq!(
            changed.record.next_run_at(),
            (kind == EventKind::Retry).then_some(TimestampMillis::new(3_000))
        );
    }
    Ok(())
}

#[test]
fn independent_terminal_pipeline_preserves_success_error_success() -> TestResult {
    // Source 538a6ff12c1e8b33:576..639; original shared type and order.
    let partition = PartitionId::new(b"flow-pipeline-terminal-partition:1".to_vec())?;
    let kind = b"flow-pipeline-terminal:1";
    let records = [
        claimed(b"flow-pipeline-terminal-1:1", kind, b"pipeline-terminal")?,
        claimed(b"flow-pipeline-terminal-2:1", kind, b"pipeline-terminal")?,
        claimed(b"flow-pipeline-terminal-3:1", kind, b"pipeline-terminal")?,
    ];
    let mut commands = records
        .each_ref()
        .map(|record| complete(record, 1_001, None));
    let middle = commands[1].as_mut().map_err(|error| error.to_string())?;
    if let Operation::Complete(terminal) = &mut middle.operation {
        terminal.guard.token = LeaseToken::new(b"wrong-token".to_vec())?;
    }
    let commands = commands.into_iter().collect::<TestResult<Vec<_>>>()?;
    let plan = accepted(decide(
        &partition,
        &records.each_ref(),
        &commands,
        &[],
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 3);
    assert_eq!(plan.results()[1], Err(DomainError::StaleLease));
    assert_eq!(records[1].state(), b"running");
    assert_eq!(records[1].version(), 2);
    for index in [0, 2] {
        let successful = success(plan.results(), index)?;
        let changed = write(successful)?;
        assert_eq!(changed.record.id(), records[index].id());
        assert_eq!(changed.record.state(), b"completed");
        assert_eq!(changed.expected_version, Some(2));
        assert_eq!(changed.record.version(), 3);
        assert_eq!(
            successful.plan().history().ok_or("history missing")?.kind,
            EventKind::Completed
        );
    }
    Ok(())
}

#[test]
fn independent_terminal_duplicate_complete_is_one_write() -> TestResult {
    // Source e0a28881c94ae756:688..732; type/id are separate uid calls.
    let partition = PartitionId::new(b"flow-pipeline-terminal-dup-partition:1".to_vec())?;
    let record = claimed(
        b"flow-pipeline-terminal-dup:2",
        b"flow-pipeline-terminal-dup:1",
        b"pipeline-terminal-dup",
    )?;
    let command = complete(&record, 1_001, None)?;
    let commands = [command.clone(), command];
    let plan = accepted(decide(
        &partition,
        &[&record],
        &commands,
        &[],
        AdmissionLimits::default(),
    )?)?;
    assert_eq!(plan.results().len(), 2);
    let first = success(plan.results(), 0)?;
    assert_eq!(first.plan().outcome(), Outcome::Changed);
    let changed = write(first)?;
    assert_eq!(changed.expected_version, Some(2));
    assert_eq!(changed.record.version(), 3);
    assert_eq!(changed.record.state(), b"completed");
    let second = success(plan.results(), 1)?;
    assert_eq!(second.plan().outcome(), Outcome::LegacyNoop);
    assert!(second.plan().write().is_none());
    assert!(second.plan().history().is_none());
    // Emitted events only: this does not query a physical history store.
    assert_eq!(
        plan.results()
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .filter_map(|result| result.plan().history())
            .filter(|event| event.kind == EventKind::Completed)
            .count(),
        1
    );
    assert_transition_profile_unchanged(&partition, &record, &commands)?;
    Ok(())
}

fn assert_transition_profile_unchanged(
    partition: &PartitionId,
    record: &WorkflowRecord,
    commands: &[CommittedCommand],
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
    let items: Vec<_> = commands
        .iter()
        .map(|c| BatchItem::new(partition, c, b"k"))
        .collect();
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
        IndependentTransitionDecision::Rejected(BatchRejection::Domain(
            DomainError::UnsupportedOperation
        ))
    );
    Ok(())
}

fn owned_pair(cap: usize, value: Option<Vec<u8>>) -> TestResult<IndependentBatchDecision> {
    let partition = PartitionId::new(b"p".to_vec())?;
    let record = claimed(b"i", b"r", b"w")?;
    let command = complete(&record, 2_000, value)?;
    let commands = [command.clone(), command];
    let entry = Vec::new();
    // Every input/current/map/entry donor drops at this return.
    decide(
        &partition,
        &[&record],
        &commands,
        &entry,
        AdmissionLimits::new(256, cap, 2)?,
    )
}

#[test]
fn independent_terminal_limits_and_owned_results() -> TestResult {
    // Independent grammar arithmetic BEFORE SUT: fixed2304 +4*(21+21)=2472.
    // Both results256: V=(260+17)+(260+273)=810 ->5544, even for eventual noop.
    for (cap, with_value, accepts) in [
        (2_472, false, true),
        (2_471, false, false),
        (5_544, true, true),
        (5_543, true, false),
    ] {
        let decision = owned_pair(cap, with_value.then(|| vec![7; 256]))?;
        if accepts {
            let results = accepted(decision)?.into_results();
            assert_eq!(results.len(), 2);
            let first = success(&results, 0)?;
            assert_eq!(first.partition().as_bytes(), b"p");
            let changed = write(first)?;
            assert_eq!(changed.record.id().as_bytes(), b"i");
            assert_eq!(changed.record.workflow_type(), b"r");
            assert_eq!(changed.record.run_state(), Some(b"queued".as_slice()));
            assert_eq!(
                changed.record.result(),
                with_value.then_some([7; 256].as_slice())
            );
            assert_eq!(changed.record.state(), b"completed");
            assert_eq!(changed.record.version(), 3);
            assert_eq!(
                first.plan().history().ok_or("history missing")?.state,
                b"completed"
            );
            let second = success(&results, 1)?.plan();
            assert_eq!(second.outcome(), Outcome::LegacyNoop);
            assert!(second.write().is_none());
            assert!(second.history().is_none());
        } else {
            assert_eq!(
                decision,
                IndependentBatchDecision::Rejected(BatchRejection::Admission(
                    AdmissionError::DecisionBytes
                ))
            );
        }
    }
    for value in [None, Some(Vec::new()), Some(vec![0, 255])] {
        let results = accepted(owned_pair(10_000, value.clone())?)?.into_results();
        assert_eq!(
            write(success(&results, 0)?)?.record.result(),
            value.as_deref()
        );
    }
    assert_empty_and_operation_families()?;
    Ok(())
}

fn assert_empty_and_operation_families() -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    assert_eq!(
        accepted(decide(
            &partition,
            &[],
            &[],
            &[],
            AdmissionLimits::default()
        )?)?
        .into_results(),
        [] as [Result<ScopedPlan, DomainError>; 0]
    );
    let record = claimed(b"i", b"r", b"w")?;
    let operations = [
        Operation::Transition(Transition {
            from: b"running".to_vec(),
            to: b"ready".to_vec(),
            fence: record.fence(),
            lease_token: Some(guard(&record)?.token),
            run_at: None,
            payload: None,
        }),
        Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(&record)?,
            run_at: None,
            policy_override: RetryOverride::default(),
            jitter: JitterSample::new(0),
            payload: None,
            error: None,
        }),
    ];
    for operation in operations {
        let command = CommittedCommand {
            id: record.id().clone(),
            now: TimestampMillis::new(2_000),
            operation,
        };
        assert_eq!(
            decide(
                &partition,
                &[&record],
                &[command],
                &[],
                AdmissionLimits::default()
            )?,
            IndependentBatchDecision::Rejected(BatchRejection::Domain(
                DomainError::UnsupportedOperation
            ))
        );
    }
    Ok(())
}
