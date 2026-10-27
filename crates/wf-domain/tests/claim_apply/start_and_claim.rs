//! Atomic start source observations; prepared policy and saved indexes are fixtures.

use super::fifo_indexes::{FifoMembers, ParallelMembers, Records, publish_checked, with_published};
use super::*;
use std::collections::BTreeSet;
use wf_domain::history::EventKind;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::value::{DomainError, InputField};
use wf_domain::workflow::batch::{BatchDecision, BatchItem, BatchRejection, decide_group_batch};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedDecision, decide_indexed_batch,
};
use wf_domain::workflow::{MAX_EXACT_MILLIS, StartAndClaim, StepContinue};

mod fifo;

const ORDER_PAYLOAD: &[u8] = b"FSV2\x02\x83t\0\0\0\x01w\x08order_ida\x01";
const RETRY_ERROR: &[u8] = b"FSV2\x02\x83t\0\0\0\x01w\x06reasonm\0\0\0\x09try-again";

type ScalarCase = (
    &'static [u8],
    &'static [u8],
    &'static [u8],
    u64,
    u64,
    DomainError,
);

const SCALAR_CASES: &[ScalarCase] = &[
    (
        b"",
        b"running",
        b"",
        0,
        0,
        DomainError::EmptyValue(InputField::Type),
    ),
    (
        b"__ferricstore_schedule",
        b"running",
        b"",
        0,
        0,
        DomainError::ReservedType,
    ),
    (
        b"t",
        b"",
        b"",
        0,
        0,
        DomainError::EmptyValue(InputField::State),
    ),
    (b"t", b"running", b"", 0, 0, DomainError::RunningTransition),
    (
        b"t",
        b"completed",
        b"",
        0,
        0,
        DomainError::TerminalTransition,
    ),
    (b"t", b"failed", b"", 0, 0, DomainError::TerminalTransition),
    (
        b"t",
        b"cancelled",
        b"",
        0,
        0,
        DomainError::TerminalTransition,
    ),
    (
        b"t",
        b"q",
        b"",
        0,
        0,
        DomainError::EmptyValue(InputField::Worker),
    ),
    (b"t", b"q", b"w", 0, 0, DomainError::ZeroLease),
    (
        b"t",
        b"q",
        b"w",
        0,
        MAX_EXACT_MILLIS + 1,
        DomainError::LeaseTooLarge,
    ),
    (
        b"t",
        b"q",
        b"w",
        MAX_EXACT_MILLIS,
        1,
        DomainError::LeaseDeadlineTooLarge,
    ),
];

fn source_start_command(
    id: &[u8],
    workflow_type: &[u8],
    state: &[u8],
    worker: &[u8],
    now: u64,
    lease_ms: u64,
    payload: Option<&[u8]>,
) -> Result<CommittedCommand, TestError> {
    Ok(CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(now),
        operation: Operation::StartAndClaim(StartAndClaim {
            workflow_type: workflow_type.to_vec(),
            state: state.to_vec(),
            worker: worker.to_vec(),
            lease_ms,
            payload: payload.map(<[u8]>::to_vec),
        }),
    })
}

fn source_start_policy(
    workflow_type: &[u8],
    fifo_charge: bool,
) -> Result<PolicyCandidate, TestError> {
    let mut patch = PolicyPatch::default();
    if fifo_charge {
        let modes = [
            (b"queued".as_slice(), StateMode::Parallel),
            (b"charge", StateMode::Fifo),
        ];
        for (state, mode) in modes {
            patch.states.insert(
                state.to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(mode),
                    ..StatePolicyPatch::default()
                }),
            );
        }
    }
    Ok(prepare_install(
        None,
        0,
        &InstallCommand::low_level(workflow_type.to_vec(), patch),
        PolicyLimits::default(),
    )?)
}

fn publish_start_source(
    scope: (&[u8], &[u8]),
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> Result<WorkflowRecord, TestError> {
    publish_checked(
        scope,
        rows,
        fifo,
        parallel,
        policy,
        (index, command, b"charge"),
        |previous, plan| {
            let changed = plan.batch().items()[0].plan();
            let write = changed.write().ok_or("atomic source write absent")?;
            let history = changed.history().ok_or("atomic source history absent")?;
            let (kind, enters) = match &command.operation {
                Operation::StartAndClaim(_) | Operation::Create(_) => (EventKind::Created, true),
                Operation::Claim(_) => (EventKind::Claimed, false),
                Operation::Retry(_) => (EventKind::Retry, false),
                _ => return Err("unexpected atomic source publication".into()),
            };
            let sequence = if enters {
                u128::from(index) << 32
            } else {
                previous
                    .ok_or("atomic source predecessor absent")?
                    .1
                    .state_enter_seq
            };
            let metadata = plan
                .metadata()
                .next()
                .ok_or("atomic source metadata absent")?;
            if (metadata.priority, metadata.state_enter_seq) != (0, sequence)
                || plan.consumed_ordinals() != u32::from(enters)
                || history.kind != kind
                || history.version != write.record.version()
                || history.fence != write.record.fence()
                || history.occurred_at != command.now
                || write.record.latest_event_witness() != (kind, history.version, history.fence)
            {
                return Err("atomic source metadata/history differs".into());
            }
            let delta = plan.deltas().next().ok_or("atomic source delta absent")?;
            for (row, record) in [
                (delta.before, previous.map(|(record, _)| record)),
                (delta.after, Some(&write.record)),
            ] {
                match (row, record) {
                    (None, None) => {}
                    (Some(IndexRows::Parallel { due }), Some(record))
                        if (due.workflow_type, due.partition_key) == scope
                            && due.id == command.id.as_bytes()
                            && due.state == record.run_state().unwrap_or(record.state())
                            && Some(due.due) == record.next_run_at()
                            && due.priority == 0 => {}
                    _ => return Err("atomic source delta differs from actual record".into()),
                }
            }
            Ok(write.record.clone())
        },
    )
}

fn reject_start_source(
    scope: (&[u8], &[u8]),
    rows: &Records,
    fifo: &FifoMembers,
    parallel: &ParallelMembers,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> Result<IndexRejection, TestError> {
    with_published(scope, rows, fifo, parallel, policy, index - 1, |view, _| {
        let partition = PartitionId::new(scope.1.to_vec())?;
        let items = [IndexedBatchItem::new(BatchItem::new(
            &partition,
            command,
            command.id.as_bytes(),
        ))];
        match decide_indexed_batch(
            view,
            &items,
            EntryOrder {
                identity: LogIdentity::new(1, 7, AppliedPosition::new(index)),
                first_ordinal: 0,
            },
            b"atomic-start-source-entry",
            AdmissionLimits::default(),
        )? {
            IndexedDecision::Rejected(rejection) => Ok(rejection),
            IndexedDecision::Accepted(_) => Err("atomic source refusal accepted".into()),
        }
    })
}

#[test]
fn source_atomic_start_creates_leased_running_execution() -> TestResult {
    // create_running_validation_test.exs33–50, declaration23e4645b7b46c5b7.
    let workflow_type = b"running-validation".as_slice();
    let scope = (workflow_type, b"running-validation-claim".as_slice());
    let installed = source_start_policy(workflow_type, false)?;
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    let command = source_start_command(
        b"valid-start-claim:1",
        workflow_type,
        b"queued",
        b"validation-worker",
        1_000,
        30_000,
        None,
    )?;
    let result = publish_start_source(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        installed.policy(),
        &command,
        1,
    );
    assert!(result.is_ok(), "{result:?}");
    let record = result?;
    assert_eq!(record.state(), b"running");
    assert_eq!(record.run_state(), Some(b"queued".as_slice()));
    let lease = record.lease().ok_or("source atomic lease absent")?;
    assert_eq!(lease.worker, b"validation-worker");
    assert_ne!(lease.token.as_bytes(), []);
    assert_eq!(lease.deadline.get(), 31_000);
    assert_eq!(lease.token.as_bytes(), b"validation-worker:1000:1");
    assert_eq!(
        (record.version(), record.attempts(), record.fence().get()),
        (1, 1, 1)
    );
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(31_000)));
    assert_eq!(record.created_at(), command.now);
    assert_eq!(record.updated_at(), command.now);
    assert_eq!(
        (record.payload(), record.result(), record.error()),
        (None, None, None)
    );
    assert_eq!(
        record.latest_event_witness(),
        (EventKind::Created, 1, record.fence())
    );
    assert_eq!(rows[command.id.as_bytes()].1.state_enter_seq, 1_u128 << 32);
    assert!(fifo.is_empty());
    assert_eq!(parallel.len(), 1);
    Ok(())
}

#[test]
fn source_atomic_start_preserves_first_step_and_returned_retry_guard() -> TestResult {
    // flow_write_contract_test.exs1319–1365, declaration1186468cb4da234a.
    let workflow_type = b"contract-start-and-claim-type-1".as_slice();
    let scope = (workflow_type, b"contract-start-and-claim-1".as_slice());
    let id = b"contract-start-and-claim-id-1".as_slice();
    let installed = source_start_policy(workflow_type, false)?;
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    let command = source_start_command(
        id,
        workflow_type,
        b"reserve_inventory",
        b"contract-start-worker",
        1_000,
        7_000,
        Some(ORDER_PAYLOAD),
    )?;
    let result = publish_start_source(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        installed.policy(),
        &command,
        1,
    );
    assert!(result.is_ok(), "{result:?}");
    let record = result?;
    assert_eq!(record.id().as_bytes(), id);
    assert_eq!(record.workflow_type(), workflow_type);
    assert_eq!(record.state(), b"running");
    assert_eq!(record.run_state(), Some(b"reserve_inventory".as_slice()));
    assert_eq!(record.attempts(), 1);
    assert_eq!(record.fence().get(), 1);
    let lease = record.lease().ok_or("first-step source lease absent")?;
    assert_eq!(lease.worker, b"contract-start-worker");
    assert_ne!(lease.token.as_bytes(), []);
    assert_eq!(lease.deadline.get(), 8_000);
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(8_000)));
    assert_eq!(record.payload(), Some(ORDER_PAYLOAD));
    let retained = (rows.clone(), fifo.clone(), parallel.clone());
    let duplicate = source_start_command(
        id,
        workflow_type,
        b"reserve_inventory",
        b"contract-start-worker",
        1_000,
        30_000,
        None,
    )?;
    let refused = reject_start_source(
        scope,
        &rows,
        &fifo,
        &parallel,
        installed.policy(),
        &duplicate,
        2,
    )?;
    assert_eq!(
        refused,
        IndexRejection::Batch(BatchRejection::Domain(DomainError::AlreadyExists))
    );
    let before = (&retained.0, &retained.1, &retained.2);
    assert_eq!((&rows, &fifo, &parallel), before);
    let retry = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(1_100),
        operation: Operation::Retry(Retry {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: record.fence(),
            },
            payload: None,
            error: Some(RETRY_ERROR.to_vec()),
            run_at: RetryTime::Explicit(TimestampMillis::new(2_000)),
        }),
    };
    let retried = publish_start_source(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        installed.policy(),
        &retry,
        3,
    );
    assert!(retried.is_ok(), "{retried:?}");
    retried?;
    let read = rows.get(id);
    assert!(read.is_some());
    let (read, _) = read.ok_or("source authoritative read absent")?;
    assert_eq!(read.state(), b"reserve_inventory");
    assert_eq!(read.run_state(), None);
    assert_eq!(read.next_run_at(), Some(TimestampMillis::new(2_000)));
    assert_eq!(read.payload(), Some(ORDER_PAYLOAD));
    assert_eq!(read.error(), Some(RETRY_ERROR));
    Ok(())
}

#[test]
fn atomic_start_admits_generated_token_before_plan() -> TestResult {
    let installed = source_start_policy(b"t", false)?;
    let rows = Records::new();
    let fifo = FifoMembers::new();
    let parallel = ParallelMembers::new();
    with_published(
        (b"t", b"p"),
        &rows,
        &fifo,
        &parallel,
        installed.policy(),
        0,
        |view, _| {
            let partition = PartitionId::new(b"p".to_vec())?;
            // Copied V9 + work:0:1 N8 gives V17 and unchanged 4V+1024 = 1092.
            let token_limit = Some(AdmissionError::ValueTooLarge {
                bytes: 8,
                maximum: 7,
            });
            let cases = [
                (b"work".as_slice(), 0, 30_000, 8, 4_096, None),
                (b"work", 0, 30_000, 7, 4_096, token_limit),
                (
                    b"work",
                    0,
                    30_000,
                    8,
                    1_091,
                    Some(AdmissionError::DecisionBytes),
                ),
                (b"work", 0, 30_000, 8, 1_092, None),
                (
                    b"w",
                    0,
                    30_000,
                    6,
                    4_096,
                    Some(AdmissionError::ValueTooLarge {
                        bytes: 7,
                        maximum: 6,
                    }),
                ),
                (b"w", 0, 30_000, 7, 4_096, None),
                // w:1000:1 has N8; raw V6 + N8 gives V14 and charge1080.
                (b"w", 1_000, 1, 7, 1_080, token_limit),
                (b"w", 1_000, 1, 8, 1_080, None),
            ];
            for (worker, now, lease_ms, inline, decision, expected) in cases {
                let command = source_start_command(b"i", b"t", b"q", worker, now, lease_ms, None)?;
                let retained = command.clone();
                let items = [BatchItem::new(&partition, &command, b"i")];
                let actual = decide_group_batch(
                    view.batch,
                    &items,
                    b"typed-start-entry",
                    AdmissionLimits::new(inline, decision, 1)?,
                )?;
                if let Some(error) = expected {
                    assert_eq!(
                        actual,
                        BatchDecision::Rejected(BatchRejection::Admission(error))
                    );
                } else {
                    assert!(matches!(&actual, BatchDecision::Accepted(_)), "{actual:?}");
                    assert_custom_cap_start(actual)?;
                }
                assert_eq!(command, retained);
            }
            Ok(())
        },
    )?;
    assert!(rows.is_empty() && fifo.is_empty() && parallel.is_empty());
    Ok(())
}

#[test]
fn atomic_start_preserves_scalar_and_duplicate_error_order() -> TestResult {
    for &(kind, state, worker, now, lease, expected) in SCALAR_CASES {
        let command = source_start_command(b"i", kind, state, worker, now, lease, None)?;
        assert_eq!(reduce(None, &command), Err(expected));
    }
    let mut global = source_start_command(b"i", b"", b"", b"", 0, 0, None)?;
    global.id = RecordId::new(b"__ferricstore_schedule__:i".to_vec())?;
    global.now = TimestampMillis::new(MAX_EXACT_MILLIS + 1);
    assert_eq!(reduce(None, &global), Err(DomainError::ReservedId));
    global.id = RecordId::new(b"i".to_vec())?;
    assert_eq!(reduce(None, &global), Err(DomainError::NowTooLarge));
    let installed = source_start_policy(b"t", false)?;
    let rows = Records::new();
    let fifo = FifoMembers::new();
    let parallel = ParallelMembers::new();
    with_published(
        (b"t", b"p"),
        &rows,
        &fifo,
        &parallel,
        installed.policy(),
        0,
        |view, _| {
            let partition = PartitionId::new(b"p".to_vec())?;
            let limits = AdmissionLimits::new(7, 4_096, 1)?;
            let fresh = source_start_command(b"i", b"t", b"q", b"w", 0, 30_000, None)?;
            let actual = decide_group_batch(
                view.batch,
                &[BatchItem::new(&partition, &fresh, b"i")],
                b"typed-start-entry",
                limits,
            )?;
            let BatchDecision::Accepted(plan) = actual else {
                return Err(format!("actual inline7 creation rejected: {actual:?}").into());
            };
            assert_eq!(plan.items().len(), 1);
            let item = plan.into_items().pop().ok_or("small-worker item absent")?;
            let (outcome, write, history) = item.into_parts().1.into_parts();
            assert_eq!(outcome, Outcome::Changed);
            let write = write.ok_or("small-worker write absent")?;
            let record = write.record;
            let history = history.ok_or("small-worker history absent")?;
            assert_eq!(write.expected_version, None);
            assert_eq!(history.kind, EventKind::Created);
            let retained = (record.clone(), history.clone(), fresh.clone());
            let mut malformed = source_start_command(b"i", b"", b"running", b"", 0, 0, None)?;
            assert_eq!(
                reduce(Some(&record), &malformed),
                Err(DomainError::AlreadyExists)
            );
            malformed.id = RecordId::new(b"other".to_vec())?;
            assert_eq!(
                reduce(Some(&record), &malformed),
                Err(DomainError::NotFound)
            );
            let duplicate = source_start_command(b"i", b"t", b"q", b"work", 0, 30_000, None)?;
            let duplicate_before = duplicate.clone();
            let current = BTreeMap::from([(
                ScopedRecordId {
                    partition_key: partition.as_bytes(),
                    id: record.id().as_bytes(),
                },
                &record,
            )]);
            // Actual current16 + raw9 + generated0 gives1124 under SAME inline7/4096.
            let actual = decide_group_batch(
                GroupView {
                    records: &current,
                    ..view.batch
                },
                &[BatchItem::new(&partition, &duplicate, b"i")],
                b"typed-start-entry",
                limits,
            )?;
            assert_eq!(
                actual,
                BatchDecision::Rejected(BatchRejection::Domain(DomainError::AlreadyExists))
            );
            assert_eq!(
                (&record, &history, &fresh),
                (&retained.0, &retained.1, &retained.2)
            );
            assert_eq!(duplicate, duplicate_before);
            Ok(())
        },
    )?;
    assert!(rows.is_empty() && fifo.is_empty() && parallel.is_empty());
    Ok(())
}

fn assert_custom_cap_start(actual: BatchDecision) -> TestResult {
    let BatchDecision::Accepted(plan) = actual else {
        return Err("valid custom-cap atomic start rejected".into());
    };
    assert_eq!(plan.items().len(), 1);
    let changed = plan.items()[0].plan();
    assert_eq!(changed.outcome(), Outcome::Changed);
    let write = changed.write().ok_or("custom-cap atomic write absent")?;
    let history = changed
        .history()
        .ok_or("custom-cap atomic history absent")?;
    assert_eq!(write.expected_version, None);
    assert_eq!(history.kind, EventKind::Created);
    assert_eq!(write.record.state(), b"running");
    assert_eq!(write.record.fence().get(), 1);
    Ok(())
}
