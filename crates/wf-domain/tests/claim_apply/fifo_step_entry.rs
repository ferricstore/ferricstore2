//! Source Step entry order survives retry and later FIFO policy-mode activation.

use super::fifo_indexes::{
    FifoMembers, ParallelMembers, Records, publish_checked, update_row, with_published,
};
use super::*;
use std::collections::BTreeSet;
use wf_domain::claim::{SelectionAction, SelectionRequest, StateFilter, select};
use wf_domain::history::EventKind;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::StepContinue;
use wf_domain::workflow::indexed::{IndexRows, index_delta_for_policy_modes};

const TYPE: &[u8] = b"flow-invariant-fifo-step-entry-type:2";
const PARTITION: &[u8] = b"tenant:fifo-step-entry:1";
const IDS: [&[u8]; 2] = [
    b"fifo-step-entry-transitioning:1",
    b"fifo-step-entry-waiting:1",
];
const RETRY_ERROR: &[u8] = b"FSV2\x02\x83t\0\0\0\x01w\x06reasonm\0\0\0\x05retry";

fn create_step_entry(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    input: (&[u8], &[u8], u64, u64, u64),
) -> Result<WorkflowRecord, TestError> {
    let (id, state, now, due, index) = input;
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(now),
        operation: Operation::Create(Create {
            workflow_type: TYPE.to_vec(),
            state: state.to_vec(),
            run_at: Some(TimestampMillis::new(due)),
            payload: None,
        }),
    };
    publish_step_entry(rows, fifo, parallel, policy, &command, index)
}

fn publish_step_entry(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> Result<WorkflowRecord, TestError> {
    publish_checked(
        (TYPE, PARTITION),
        rows,
        fifo,
        parallel,
        policy,
        (index, command, b"charge"),
        |previous, plan| {
            let changed = plan.batch().items()[0].plan();
            let write = changed.write().ok_or("source step write absent")?;
            let history = changed.history().ok_or("source step history absent")?;
            let (kind, state, due, enters) = match &command.operation {
                Operation::Create(create) => (
                    EventKind::Created,
                    create.state.as_slice(),
                    create.run_at.ok_or("source create due absent")?.get(),
                    true,
                ),
                Operation::Claim(claim) => (
                    EventKind::Claimed,
                    claim.expected_state.as_slice(),
                    command.now.get() + claim.lease_ms,
                    false,
                ),
                Operation::StepContinue(step) => (
                    EventKind::StepContinued,
                    step.to.as_slice(),
                    command.now.get() + step.lease_ms,
                    true,
                ),
                Operation::Retry(retry) => {
                    assert_eq!(retry.error.as_deref(), Some(RETRY_ERROR));
                    (EventKind::Retry, b"charge".as_slice(), 2_200, false)
                }
                _ => return Err("unexpected source step-entry operation".into()),
            };
            let sequence = if enters {
                u128::from(index) << 32
            } else {
                previous
                    .ok_or("source predecessor absent")?
                    .1
                    .state_enter_seq
            };
            let metadata = plan.metadata().next().ok_or("source metadata absent")?;
            assert_eq!((metadata.priority, metadata.state_enter_seq), (0, sequence));
            assert_eq!(plan.consumed_ordinals(), u32::from(enters));
            assert_eq!(history.kind, kind);
            assert_eq!(write.record.payload(), None);
            let old_state =
                previous.map(|(record, _)| record.run_state().unwrap_or(record.state()));
            let old_due = previous
                .and_then(|(record, _)| record.next_run_at())
                .map(TimestampMillis::get);
            let delta = plan.deltas().next().ok_or("source delta absent")?;
            for (row, logical, deadline) in [
                (delta.before, old_state, old_due),
                (delta.after, Some(state), Some(due)),
            ] {
                let Some(row) = row else {
                    assert!(logical.is_none());
                    continue;
                };
                let (mode, id, state, due) = match row {
                    IndexRows::Parallel { due } => {
                        (StateMode::Parallel, due.id, due.state, Some(due.due.get()))
                    }
                    IndexRows::Fifo {
                        lane,
                        member,
                        head_candidate,
                    } => (
                        StateMode::Fifo,
                        member.id(),
                        lane.state,
                        head_candidate.map(|due| due.due.get()),
                    ),
                };
                assert_eq!(id, command.id.as_bytes());
                assert_eq!((Some(state), due), (logical, deadline));
                assert_eq!(mode, policy.state_mode(state));
            }
            Ok(write.record.clone())
        },
    )
}

fn claim_step_entry(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    step: (u64, u64, &[u8]),
    worker: &[u8],
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let (now, index, state) = step;
    let partitions = [PARTITION];
    let states = [state];
    let request = SelectionRequest {
        partitions: PartitionFilter::Only(&partitions),
        states: StateFilter::Only(&states),
        limit: 1,
        ..SelectionRequest::new(b"g", TYPE, TimestampMillis::new(now))
    };
    let (count, selected) = with_published(
        (TYPE, PARTITION),
        rows,
        fifo,
        parallel,
        policy,
        index - 1,
        |_, view| {
            assert_eq!((request.group, request.workflow_type), (view.group, TYPE));
            let selection = select(&view, request, None)?;
            let count = selection.records().len();
            if count != 1 {
                return Ok((count, None));
            }
            let selected = selection.records()[0];
            assert_eq!(selected.partition_key, PARTITION);
            assert_eq!(selected.action, SelectionAction::Claim);
            assert_eq!(selected.record.state(), state);
            let fence = selected
                .record
                .fence()
                .checked_next()
                .ok_or("claim fence overflow")?;
            Ok((count, Some((selected.record.id().clone(), fence))))
        },
    )?;
    let Some((id, fence)) = selected else {
        return Ok((count, None));
    };
    let mut token = worker.to_vec();
    token.extend_from_slice(format!(":{now}:{}", fence.get()).as_bytes());
    let command = CommittedCommand {
        id,
        now: TimestampMillis::new(now),
        operation: Operation::Claim(Claim {
            expected_state: state.to_vec(),
            worker: worker.to_vec(),
            token: LeaseToken::new(token)?,
            lease_ms: 30_000,
        }),
    };
    let record = publish_step_entry(rows, fifo, parallel, policy, &command, index)?;
    let Operation::Claim(claim) = &command.operation else {
        return Err("source claim command absent".into());
    };
    let lease = record.lease().ok_or("source claimed lease absent")?;
    assert_eq!(record.version(), 2);
    assert_eq!(
        (record.state(), record.run_state()),
        (b"running".as_slice(), Some(state))
    );
    assert_eq!(lease.worker, worker);
    assert_eq!(lease.token, claim.token);
    assert_eq!(
        (record.fence(), lease.deadline.get()),
        (fence, now + 30_000)
    );
    Ok((count, Some(record)))
}

fn continue_step_entry(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    claimed: &WorkflowRecord,
) -> Result<WorkflowRecord, TestError> {
    let command = CommittedCommand {
        id: claimed.id().clone(),
        now: TimestampMillis::new(2_100),
        operation: Operation::StepContinue(StepContinue {
            guard: LeaseGuard {
                token: claimed
                    .lease()
                    .ok_or("source claim lease absent")?
                    .token
                    .clone(),
                fence: claimed.fence(),
            },
            from: b"reserve".to_vec(),
            to: b"charge".to_vec(),
            worker: Some(b"parallel-worker".to_vec()),
            lease_ms: 30_000,
            payload: None,
        }),
    };
    let record = publish_step_entry(rows, fifo, parallel, policy, &command, 4)?;
    let lease = record.lease().ok_or("source continued lease absent")?;
    assert_eq!((record.version(), record.fence().get()), (3, 2));
    assert_eq!(
        (record.state(), record.run_state()),
        (b"running".as_slice(), Some(b"charge".as_slice()))
    );
    assert_eq!(lease.token.as_bytes(), b"parallel-worker:2100:2");
    assert_eq!(lease.deadline.get(), 32_100);
    assert_eq!(lease.worker, b"parallel-worker");
    Ok(record)
}

fn retry_step_entry(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    continued: &WorkflowRecord,
) -> TestResult {
    let command = CommittedCommand {
        id: continued.id().clone(),
        now: TimestampMillis::new(2_101),
        operation: Operation::Retry(Retry {
            guard: LeaseGuard {
                token: continued
                    .lease()
                    .ok_or("source continued lease absent")?
                    .token
                    .clone(),
                fence: continued.fence(),
            },
            run_at: RetryTime::Explicit(TimestampMillis::new(2_200)),
            payload: None,
            error: Some(RETRY_ERROR.to_vec()),
        }),
    };
    assert_eq!(RETRY_ERROR.len(), 29);
    let record = publish_step_entry(rows, fifo, parallel, policy, &command, 5)?;
    assert_eq!(
        (record.version(), record.fence().get(), record.attempts()),
        (4, 2, 1)
    );
    assert_eq!(
        (record.state(), record.run_state()),
        (b"charge".as_slice(), None)
    );
    assert_eq!(record.lease(), None);
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(2_200)));
    assert_eq!(record.error(), Some(RETRY_ERROR));
    Ok(())
}

fn activate_step_charge(
    rows: &Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    old_policy: &PolicySnapshot,
) -> Result<PolicyCandidate, TestError> {
    let patch = PolicyPatch {
        states: BTreeMap::from([(
            b"charge".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    let candidate = prepare_install(
        None,
        0,
        &InstallCommand::low_level(TYPE.to_vec(), patch),
        PolicyLimits::default(),
    )?;
    let mut next_fifo = fifo.clone();
    let mut next_parallel = parallel.clone();
    with_published(
        (TYPE, PARTITION),
        rows,
        fifo,
        parallel,
        old_policy,
        5,
        |view, _| {
            assert_eq!(view.records.len(), 2);
            assert_eq!(candidate.policy().workflow_type(), TYPE);
            for entry in view.records.values() {
                assert_eq!(entry.logical_state(), b"charge");
                let before = old_policy.state_mode(entry.logical_state());
                let after = candidate.policy().state_mode(entry.logical_state());
                assert_eq!((before, after), (StateMode::Parallel, StateMode::Fifo));
                let delta = index_delta_for_policy_modes(*entry, before, after);
                let due = DueOrder::for_record(entry).ok_or("activation due absent")?;
                assert_eq!(delta.before, Some(IndexRows::Parallel { due }));
                assert_eq!(
                    delta.after,
                    Some(IndexRows::Fifo {
                        lane: LaneKey::for_record(entry).ok_or("activation lane absent")?,
                        member: LaneOrder::for_record(entry),
                        head_candidate: Some(due),
                    })
                );
                for (row, insert) in [(delta.before, false), (delta.after, true)] {
                    let row = row.ok_or("activation mode delta absent")?;
                    let id = match row {
                        IndexRows::Fifo { member, .. } => member.id(),
                        IndexRows::Parallel { due } => due.id,
                    };
                    assert_eq!(id, entry.record.id().as_bytes());
                    update_row(
                        (TYPE, PARTITION),
                        b"charge",
                        row,
                        insert,
                        &mut next_fifo,
                        &mut next_parallel,
                    )?;
                }
            }
            Ok(())
        },
    )?;
    *fifo = next_fifo;
    *parallel = next_parallel;
    Ok(candidate)
}

#[test]
fn fifo_step_continue_preserves_entry_before_activation() -> TestResult {
    // invariant_test.exs850–915, declarationf2f7a6b246087607; pure sequence only.
    let initial = prepare_install(
        None,
        0,
        &InstallCommand::low_level(TYPE.to_vec(), PolicyPatch::default()),
        PolicyLimits::default(),
    )?;
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    let created = create_step_entry(
        &mut rows,
        &mut fifo,
        &mut parallel,
        initial.policy(),
        (IDS[0], b"reserve", 1_000, 2_000, 1),
    );
    assert!(created.is_ok(), "{created:?}");
    created?;
    let created = create_step_entry(
        &mut rows,
        &mut fifo,
        &mut parallel,
        initial.policy(),
        (IDS[1], b"charge", 1_001, 2_200, 2),
    );
    assert!(created.is_ok(), "{created:?}");
    created?;
    let (first_count, first) = claim_step_entry(
        &mut rows,
        &mut fifo,
        &mut parallel,
        initial.policy(),
        (2_000, 3, b"reserve"),
        b"parallel-worker",
    )?;
    assert_eq!(
        (
            first_count,
            first.as_ref().map(|record| record.id().as_bytes())
        ),
        (1, Some(IDS[0]))
    );
    let claimed = first.ok_or("source reserve selection absent")?;
    let continued = continue_step_entry(
        &mut rows,
        &mut fifo,
        &mut parallel,
        initial.policy(),
        &claimed,
    );
    assert!(continued.is_ok(), "{continued:?}");
    let continued = continued?;
    let retried = retry_step_entry(
        &mut rows,
        &mut fifo,
        &mut parallel,
        initial.policy(),
        &continued,
    );
    assert!(retried.is_ok(), "{retried:?}");
    retried?;
    let before_activation = rows.clone();
    let activated = activate_step_charge(&rows, &mut fifo, &mut parallel, initial.policy())?;
    assert_eq!(rows, before_activation);
    assert_eq!(
        fifo,
        BTreeSet::from([
            (0, 2_u128 << 32, IDS[1].to_vec()),
            (0, 4_u128 << 32, IDS[0].to_vec()),
        ])
    );
    assert!(parallel.is_empty());
    let (last_count, last) = claim_step_entry(
        &mut rows,
        &mut fifo,
        &mut parallel,
        activated.policy(),
        (2_200, 7, b"charge"),
        b"fifo-worker",
    )?;
    assert_eq!(
        (
            last_count,
            last.as_ref().map(|record| record.id().as_bytes())
        ),
        (1, Some(IDS[1]))
    );
    assert_eq!(rows[IDS[0]], before_activation[IDS[0]]);
    assert_eq!(
        fifo,
        BTreeSet::from([
            (-1, 2_u128 << 32, IDS[1].to_vec()),
            (0, 4_u128 << 32, IDS[0].to_vec()),
        ])
    );
    assert!(parallel.is_empty());
    Ok(())
}
