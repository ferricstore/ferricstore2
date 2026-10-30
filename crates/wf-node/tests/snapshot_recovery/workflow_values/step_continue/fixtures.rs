use super::*;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome};
use wf_domain::workflow::{Operation, StartAndClaim, StepContinue};
use wf_ports::consensus::{PreparedOperation, PreparedRequest, WorkflowValuePolicyWitness};
use wf_ports::log::encode_step_continue;
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StoredWorkflowRecord, WorkflowHistoryDirection,
    WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};
use wf_storage::state::{StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_step_continue_transaction};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::Fence;

pub(super) fn limits() -> TestResult<Limits> {
    let selected =
        v::limits()?.with_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        selected.is_ok(),
        "healthy Step recovery opt-in: {:?}",
        selected.as_ref().err()
    );
    Ok(selected?)
}

pub(super) fn pair(dir: &Path, stored: bool) -> TestResult<(RedbLogStore, RedbStateStore)> {
    if !stored {
        return v::pair(dir);
    }
    let scope = f::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let state = RedbStateStore::create_stored_value_database(
        redb::Builder::new().create(dir.join("state.redb"))?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0, 0, 0)?,
            placements: &placements,
            policies: &[],
        },
        v::defaults(7000),
        StateStoreLimits::default(),
    )?;
    Ok((v::log(dir)?, state))
}

pub(super) fn seed(
    dir: &Path,
    stored: bool,
    upgraded: bool,
) -> TestResult<(RedbLogStore, RedbStateStore, WorkflowValuePolicyWitness)> {
    let (mut log, mut state) = pair(dir, stored)?;
    let policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    if upgraded {
        state = state.upgrade_history_snapshot_layout()?;
    }
    let start = start_frame(&policy)?;
    f::append(&mut log, 1, start.as_bytes())?;
    v::accepted(v::apply(&mut state, &start, 1)?, 1, 100, 7)?;
    log.save_committed(f::at(1))?;
    Ok((log, state, policy))
}

pub(super) fn start_frame(policy: &WorkflowValuePolicyWitness) -> TestResult<PreparedBytes> {
    v::encode(&v::batch(
        &f::scope()?,
        7,
        b"start",
        100,
        vec![v::item(
            b"i",
            100,
            Operation::StartAndClaim(StartAndClaim {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                worker: b"worker".to_vec(),
                lease_ms: 5000,
                payload: Some(b"old".to_vec()),
            }),
        )?],
        v::copy_policy(policy),
    )?)
}

pub(super) fn frame(request: &[u8], now: u64, payload: Option<&[u8]>) -> TestResult<PreparedBytes> {
    guarded_frame(request, now, payload, (b"worker:100:1", 1, b"q", b"charge"))
}

pub(super) fn guarded_frame(
    request: &[u8],
    now: u64,
    payload: Option<&[u8]>,
    guard: (&[u8], u64, &[u8], &[u8]),
) -> TestResult<PreparedBytes> {
    let limits = v::limits()?.normal_limits();
    Ok(encode_step_continue(
        PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(request.to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: f::clock(now, 0, now)?,
            operation: PreparedOperation::Batch(vec![v::item(
                b"i",
                now,
                Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(guard.0.to_vec())?,
                        fence: Fence::new(guard.1),
                    },
                    from: guard.2.to_vec(),
                    to: guard.3.to_vec(),
                    worker: None,
                    lease_ms: 5000,
                    payload: payload.map(<[u8]>::to_vec),
                }),
            )?]),
        },
        limits.codec,
        limits.workspace_bytes,
    )?)
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    frame: &PreparedBytes,
    at: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    Ok(apply_step_continue_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::at(at),
        v::driver(&v::limits()?),
    )?)
}

pub(super) fn pending(dir: &Path, applied: bool, stored: bool) -> TestResult<PreparedBytes> {
    let (mut log, mut state, _) = seed(dir, stored, true)?;
    let frame = frame(b"step", 200, Some(b"FSV2raw"))?;
    f::append(&mut log, 2, frame.as_bytes())?;
    if applied {
        drop(apply(&mut state, &frame, 2)?);
    }
    log.save_committed(f::at(2))?;
    drop((log, state));
    Ok(frame)
}

pub(super) fn recover(dir: &Path, limits: &Limits) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    Ok(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
    )?)
}

pub(super) fn record(state: &mut RedbStateStore) -> TestResult<StoredWorkflowRecord> {
    let scope = f::scope()?;
    let limits = v::limits()?.normal_limits();
    Ok(state
        .begin_apply(
            scope.group(),
            scope.recovery(),
            limits.codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("current Step record")?)
}

pub(super) fn history(state: &RedbStateStore) -> TestResult<WorkflowHistoryPage> {
    Ok(state.read_workflow_history_page(
        &f::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"i".to_vec())?,
        WorkflowHistoryQuery {
            limit: 100,
            boundary_event: None,
            direction: WorkflowHistoryDirection::Ascending,
            expected_control: None,
        },
        WorkflowHistoryReadLimits {
            maximum_entries: 100,
            maximum_row_reads: 10000,
            maximum_inspected_bytes: 64 * f::MIB,
            maximum_returned_bytes: 64 * f::MIB,
            maximum_work: 64 * f::MIB,
            maximum_temporary_bytes: 64 * f::MIB,
        },
    )?)
}

pub(super) fn retained(result: RecordBatchTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("actual retained Step".into());
    };
    Ok(outcome)
}

pub(super) fn cohort(dir: &Path) -> TestResult<[f::Rows; 6]> {
    let path = dir.join("state.redb");
    Ok([
        f::bytes_table(&path, "wf_state_meta_v1")?,
        f::bytes_table(&path, "wf_state_rows_v1")?,
        f::bytes_table(&path, "wf_state_snapshot_meta_v1")?,
        f::bytes_table(&path, "wf_state_snapshot_rows_v1")?,
        f::bytes_table(&path, "wf_state_snapshot_alt_meta_v1")?,
        f::bytes_table(&path, "wf_state_snapshot_alt_rows_v1")?,
    ])
}

pub(super) fn refuse(dir: &Path, limits: &Limits, expected: Option<Error>) -> TestResult {
    let before = cohort(dir)?;
    let rows = f::log_rows(dir)?;
    let meta = f::log_meta(dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    let error = recover_committed(log, state, scope.group(), scope.recovery(), *limits)
        .err()
        .ok_or("Step recovery should refuse")?;
    if let Some(expected) = expected {
        assert_eq!(error, expected);
    }
    assert_eq!(cohort(dir)?, before);
    assert_eq!(f::log_rows(dir)?, rows);
    assert_eq!(f::log_meta(dir)?, meta);
    Ok(())
}
