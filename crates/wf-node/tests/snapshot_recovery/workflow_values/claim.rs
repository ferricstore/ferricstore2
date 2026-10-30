//! Real Create/named Claim/Step/Retry on durable committed and received prefixes.

use super::{Error, Limits, TestResult, f, fixtures as v, recover_committed, recover_snapshotted};
use std::{io::Cursor, path::Path};
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_domain::workflow::{Operation, StepContinue};
use wf_ports::consensus::{
    PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_ports::log::{PreparedBytes, RaftLogStore, encode_prepared, encode_step_continue};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord,
    WorkflowHistoryDirection, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};
use wf_storage::log::RedbLogStore;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::ClaimTransactionError;
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

fn limits() -> TestResult<Limits> {
    let selected = v::limits()?
        .with_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?
        .with_workflow_value_claim_limits(
            ClaimWorkLimits::default(),
            1024,
            64 * f::MIB,
            64 * f::MIB,
            64 * f::MIB,
        );
    assert!(
        selected.is_ok(),
        "healthy value Claim recovery opt-in: {:?}",
        selected.as_ref().err()
    );
    Ok(selected?)
}

#[test]
fn value_claim_node_replays_real_named_chain_and_preserves_original_receipt() -> TestResult {
    let limits = limits()?;
    let dir = f::directory("value-claim-node-chain")?;
    let (log, state, _) = input(&dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), limits)?;
    verify(&mut state)?;
    assert_eq!(log.state()?.committed(), Some(f::at(4)));
    assert_eq!(log.state()?.last_log(), Some(f::at(5)));
    let original = v::outcome(&mut state, b"claim")?;
    assert_eq!(
        (
            original.intent().version(),
            original.response_version(),
            original.original_position(),
            original.original_time().get()
        ),
        (1, 3, f::at(2), 200)
    );
    assert_eq!(original.status(), RetainedStatus::Accepted);
    drop((log, state));
    let before = cohort(&dir)?;
    let rows = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
    )?);
    assert_eq!(cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, rows);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn value_claim_node_old_opt_ins_and_narrow_values_leave_state_and_log_exact() -> TestResult {
    let healthy = limits()?;
    assert!(v::limits()?.workflow_value_claim_limits().is_none());
    let dir = f::directory("value-claim-node-refuse")?;
    let (log, state, _) = input(&dir)?;
    drop((log, state));
    let before = cohort(&dir)?;
    let rows = f::log_rows(&dir)?;
    let old = v::limits()?
        .with_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?
        .with_claim_work_limits(ClaimWorkLimits::default());
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), old).err(),
        Some(Error::Claim(ClaimTransactionError::Transaction(
            StateTransactionError::UnsupportedLayout
        )))
    );
    assert_eq!(cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, rows);
    let narrow = v::limits()?
        .with_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?
        .with_workflow_value_claim_limits(
            ClaimWorkLimits::default(),
            1,
            64 * f::MIB,
            64 * f::MIB,
            64 * f::MIB,
        )?;
    let (log, state) = f::open_pair(&dir)?;
    assert!(recover_committed(log, state, scope.group(), scope.recovery(), narrow).is_err());
    assert_eq!(cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, rows);
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        healthy,
    )?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn value_claim_node_snapshot_and_received_purged_suffix_restore_named_chain() -> TestResult {
    use wf_node::recovery::{purge_snapshot, restore_received_snapshot};
    let limits = limits()?;
    let dir = f::directory("value-claim-node-received")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (log, mut state, frames) = input(&source)?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    verify(&mut state)?;
    drop((log, state));
    let expected = cohort(&source)?;
    let target = dir.join("target");
    std::fs::create_dir(&target)?;
    let mut log = v::log(&target)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 5, b"uncommitted opaque tail")?;
    let candidate = RedbSnapshotStage::receive(
        &target.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.committed(), Some(f::at(1)));
    log.save_committed(f::at(4))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits,
        SnapshotLimits::default(),
    )?;
    verify(&mut state)?;
    drop((log, state));
    assert_eq!(cohort(&target)?[1], expected[1]);
    drop(f::open_pair(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn input(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore, Vec<PreparedBytes>)> {
    let (mut log, mut state) = v::pair(dir)?;
    let policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    let mut old = Vec::new();
    let captured = state.capture_snapshot(&mut old, SnapshotLimits::default())?;
    state.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    state = state.upgrade_history_snapshot_layout()?;
    let create = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"create",
        100,
        vec![v::create(b"i", 100, b"old")?],
        v::copy_policy(&policy),
    )?)?;
    f::append(&mut log, 1, create.as_bytes())?;
    v::accepted(v::apply(&mut state, &create, 1)?, 1, 100, 5)?;
    let claim = encode_prepared(
        &PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(b"claim".to_vec())?,
            intent: ClientIntent::new(1, [42; 32])?,
            stamp: f::clock(200, 0, 200)?,
            operation: PreparedOperation::ClaimPhase(PreparedClaimPhase {
                query: PreparedClaimQuery {
                    workflow_type: b"r".to_vec(),
                    states: Some(vec![b"q".to_vec()]),
                    exclude_states: Vec::new(),
                    partitions: Some(vec![b"p".to_vec()]),
                    priority: None,
                    worker: b"worker".to_vec(),
                    lease_ms: 5000,
                    limit: 1,
                },
                token_version: 1,
                first_ordinal: u32::MAX,
                scan_budget: 1000,
                head_budget: 1000,
            }),
        },
        v::limits()?.normal_limits().codec,
    )?;
    let step = encode_step_continue(
        PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(b"step".to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: f::clock(210, 0, 210)?,
            operation: PreparedOperation::Batch(vec![v::item(
                b"i",
                210,
                Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(b"worker:200:1".to_vec())?,
                        fence: Fence::new(1),
                    },
                    from: b"q".to_vec(),
                    to: b"charge".to_vec(),
                    worker: None,
                    lease_ms: 5000,
                    payload: Some(b"new".to_vec()),
                }),
            )?]),
        },
        v::limits()?.normal_limits().codec,
        512 * f::MIB,
    )?;
    let retry = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"retry",
        220,
        vec![v::item(
            b"i",
            220,
            Operation::Retry(Retry {
                guard: LeaseGuard {
                    token: LeaseToken::new(b"worker:210:2".to_vec())?,
                    fence: Fence::new(2),
                },
                run_at: RetryTime::Explicit(TimestampMillis::new(400)),
                payload: None,
                error: Some(b"again".to_vec()),
            }),
        )?],
        v::copy_policy(&policy),
    )?)?;
    let frames = vec![
        v::policy_frame(false, None, 0, 10)?,
        create,
        claim,
        step,
        retry,
    ];
    for (index, frame) in frames.iter().enumerate().skip(2) {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 5, b"uncommitted opaque tail")?;
    log.save_committed(f::at(4))?;
    Ok((log, state, frames))
}

fn verify(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let current: StoredWorkflowRecord = state
        .begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits()?.normal_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("named chain record")?;
    assert_eq!(
        (
            current.record.version(),
            current.record.fence().get(),
            current.record.attempts()
        ),
        (4, 2, 1)
    );
    assert_eq!(current.record.state(), b"charge");
    assert_eq!(current.record.run_state(), None);
    assert_eq!(
        current.record.next_run_at(),
        Some(TimestampMillis::new(400))
    );
    assert_eq!(current.record.payload(), Some(b"new".as_slice()));
    assert_eq!(current.record.error(), Some(b"again".as_slice()));
    let history = state.read_workflow_history_page(
        &scope,
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
    )?;
    assert_eq!(history.entries.len(), 4);
    assert_eq!(v::control(state)?.position(), f::at(4).index());
    Ok(())
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
