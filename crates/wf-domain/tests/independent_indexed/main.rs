//! General independent publication plans; persistence and public profiles are separate.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, PolicySnapshot, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{
    InstallCommand, PolicyCandidate, PolicyLimits, prepare_install,
};
use wf_domain::policy::{InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{
    ApplyError, BatchItem, BatchRejection, GroupView, IndependentTransitionDecision, ScopedPlan,
    decide_independent_terminal_batch, decide_independent_transition_batch,
};
use wf_domain::workflow::indexed::{
    EntryOrder, IndependentIndexedDecision, IndependentIndexedPlan, IndexRejection, IndexRows,
    TouchedIndexedGroupView, decide_touched_independent_indexed_batch,
};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, DomainError, Operation, Outcome, RecordWrite, Terminal,
    Transition, WorkflowRecord, reduce,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

mod fold;
mod publication;

fn command(id: &[u8], operation: Operation) -> TestResult<CommittedCommand> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(2_000),
        operation,
    })
}

fn create(id: &[u8], state: &[u8]) -> TestResult<CommittedCommand> {
    command(
        id,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: state.to_vec(),
            run_at: Some(TimestampMillis::new(1_000)),
            payload: None,
        }),
    )
}

fn initial(id: &[u8], state: &[u8]) -> TestResult<WorkflowRecord> {
    let mut input = create(id, state)?;
    input.now = TimestampMillis::new(1_000);
    Ok(reduce(None, &input)?
        .into_parts()
        .1
        .ok_or("create absent")?
        .record)
}

fn claimed(id: &[u8]) -> TestResult<WorkflowRecord> {
    let before = initial(id, b"queued")?;
    let mut input = command(
        id,
        Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"k".to_vec())?,
            lease_ms: 30_000,
        }),
    )?;
    input.now = TimestampMillis::new(1_000);
    Ok(reduce(Some(&before), &input)?
        .into_parts()
        .1
        .ok_or("claim absent")?
        .record)
}

fn guard(record: &WorkflowRecord) -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        fence: record.fence(),
        token: record.lease().ok_or("lease absent")?.token.clone(),
    })
}

fn retry(record: &WorkflowRecord, payload: Option<Vec<u8>>) -> TestResult<CommittedCommand> {
    command(
        record.id().as_bytes(),
        Operation::Retry(Retry {
            guard: guard(record)?,
            run_at: RetryTime::Explicit(TimestampMillis::new(3_000)),
            payload,
            error: None,
        }),
    )
}

fn cancel(id: &[u8], fence: u64) -> TestResult<CommittedCommand> {
    command(
        id,
        Operation::Cancel(Cancel {
            fence: Fence::new(fence),
            lease_token: None,
            expected_state: None,
            expected_version: None,
            error: None,
        }),
    )
}

fn transition(id: &[u8], from: &[u8], to: &[u8]) -> TestResult<CommittedCommand> {
    command(
        id,
        Operation::Transition(Transition {
            from: from.to_vec(),
            to: to.to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: None,
        }),
    )
}

fn complete(record: &WorkflowRecord) -> TestResult<CommittedCommand> {
    command(
        record.id().as_bytes(),
        Operation::Complete(Terminal {
            guard: guard(record)?,
            payload: None,
            value: None,
        }),
    )
}

fn entry_order(index: u64, first_ordinal: u32) -> EntryOrder {
    EntryOrder {
        identity: LogIdentity::new(3, 7, AppliedPosition::new(index)),
        first_ordinal,
    }
}

fn entry<'a>(
    partition: &'a PartitionId,
    record: &'a WorkflowRecord,
    priority: u8,
) -> ClaimRecord<'a> {
    ClaimRecord {
        record,
        partition_key: partition.as_bytes(),
        priority,
        state_enter_seq: 99,
    }
}

fn policy(states: &[(&[u8], StateMode)]) -> TestResult<PolicyCandidate> {
    let states = states
        .iter()
        .map(|(name, mode)| {
            (
                name.to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(*mode),
                    ..StatePolicyPatch::default()
                }),
            )
        })
        .collect();
    Ok(prepare_install(
        None,
        0,
        &InstallCommand::low_level(
            b"r".to_vec(),
            PolicyPatch {
                states,
                ..PolicyPatch::default()
            },
        ),
        PolicyLimits::default(),
    )?)
}

fn mode(policy: Option<&PolicySnapshot>, row: ClaimRecord<'_>) -> StateMode {
    policy.map_or(StateMode::Parallel, |value| {
        value.state_mode(row.logical_state())
    })
}

fn with_view<'current, T>(
    rows: &[ClaimRecord<'current>],
    commands: &[(&PartitionId, &CommittedCommand, u8)],
    policy: Option<&PolicySnapshot>,
    retry: Option<&InstalledRetryPolicy>,
    inspect: impl FnOnce(
        TouchedIndexedGroupView<'_, 'current>,
        &[wf_domain::workflow::indexed::IndexedBatchItem<'_>],
    ) -> TestResult<T>,
) -> TestResult<T> {
    let group = GroupId::new(b"g".to_vec())?;
    let placements = commands
        .iter()
        .map(|(p, _, _)| ((*p).clone(), group.clone()))
        .collect();
    let mut current: BTreeMap<_, _> = rows
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), Some(*row)))
        .collect();
    let mut policies = BTreeMap::new();
    let mut retries = BTreeMap::new();
    for row in rows {
        policies.insert(row.record.workflow_type(), policy);
        retries.insert(row.record.workflow_type(), retry);
    }
    for (partition, command, _) in commands {
        current
            .entry(ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: command.id.as_bytes(),
            })
            .or_insert(None);
        if let Operation::Create(value) = &command.operation {
            policies.insert(value.workflow_type.as_slice(), policy);
            retries.insert(value.workflow_type.as_slice(), retry);
        }
    }
    let parallel = rows
        .iter()
        .filter_map(|row| {
            DueOrder::for_record(row).map(|key| (key, mode(policy, *row) == StateMode::Parallel))
        })
        .collect();
    let members = rows
        .iter()
        .filter_map(|row| {
            LaneKey::for_record(row).map(|lane| {
                (
                    (lane, LaneOrder::for_record(row)),
                    (mode(policy, *row) == StateMode::Fifo).then_some(*row),
                )
            })
        })
        .collect();
    let heads = rows
        .iter()
        .filter_map(|row| {
            LaneKey::for_record(row).map(|lane| {
                let head = rows
                    .iter()
                    .filter(|other| {
                        mode(policy, **other) == StateMode::Fifo
                            && LaneKey::for_record(other) == Some(lane)
                    })
                    .min_by_key(|other| LaneOrder::for_record(other));
                (lane, head.map(|head| (LaneOrder::for_record(head), *head)))
            })
        })
        .collect();
    let items: Vec<_> = commands
        .iter()
        .map(
            |(partition, command, priority)| wf_domain::workflow::indexed::IndexedBatchItem {
                item: BatchItem::new(partition, command, b"k"),
                create_priority: *priority,
            },
        )
        .collect();
    inspect(
        TouchedIndexedGroupView {
            group: &group,
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &policies,
            retry_policies: &retries,
        },
        &items,
    )
}

fn decide<'current>(
    rows: &[ClaimRecord<'current>],
    commands: &[(&PartitionId, &CommittedCommand, u8)],
    policy: Option<&PolicySnapshot>,
    retry: Option<&InstalledRetryPolicy>,
    limits: AdmissionLimits,
) -> TestResult<IndependentIndexedDecision<'current>> {
    with_view(rows, commands, policy, retry, |view, items| {
        Ok(decide_touched_independent_indexed_batch(
            view,
            items,
            entry_order(7, 5),
            &[],
            limits,
        )?)
    })
}

fn accepted(decision: IndependentIndexedDecision<'_>) -> TestResult<IndependentIndexedPlan<'_>> {
    match decision {
        IndependentIndexedDecision::Accepted(plan) => Ok(plan),
        IndependentIndexedDecision::Rejected(reason) => {
            Err(format!("whole rejection: {reason:?}").into())
        }
    }
}

fn success(
    results: &[Result<ScopedPlan, IndexRejection>],
    index: usize,
) -> TestResult<&ScopedPlan> {
    Ok(results
        .get(index)
        .ok_or("result absent")?
        .as_ref()
        .map_err(|e| format!("item rejection: {e:?}"))?)
}

fn write(plan: &ScopedPlan) -> TestResult<&RecordWrite> {
    Ok(plan.plan().write().ok_or("write absent")?)
}

fn rejected(error: DomainError) -> IndexRejection {
    IndexRejection::Batch(BatchRejection::Domain(error))
}
