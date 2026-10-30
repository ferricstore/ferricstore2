use super::*;
use std::path::Path;
use wf_ports::state::{
    WorkflowHistoryDirection, WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};

pub(super) fn limits() -> WorkflowValueTransactionLimits {
    let mut limits = values::limits();
    limits.record_batch.outcome_ttl_ms = 100;
    limits
}

pub(super) fn upgrade(store: RedbStateStore) -> TestResult<RedbStateStore> {
    let result = store.upgrade_history_snapshot_layout();
    assert!(
        result.is_ok(),
        "healthy history layout upgrade: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn fresh(path: &Path) -> TestResult<RedbStateStore> {
    upgrade(values::fresh(path)??)
}

pub(super) fn seed(store: &mut RedbStateStore) -> TestResult {
    let request = values::batch(
        7,
        b"seed",
        0,
        PreparedOperation::Batch(vec![values::item(
            b"i",
            0,
            Operation::StartAndClaim(StartAndClaim {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                worker: b"w".to_vec(),
                lease_ms: 30_000,
                payload: Some(b"old".to_vec()),
            }),
        )?]),
        vec![values::input(0, 0)],
        vec![values::witness(b"r", None)?],
    )?;
    let outcome = retained(values::apply_value(store, &common::scope()?, &request, 0)?)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(())
}

pub(super) fn record(store: &mut RedbStateStore) -> TestResult<StoredWorkflowRecord> {
    let scope = common::scope()?;
    Ok(store
        .begin_apply(
            scope.group(),
            scope.recovery(),
            limits().record_batch.codec.admission(),
            1000,
            common::MIB,
            common::MIB,
        )?
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("current record")?)
}

pub(super) fn frame(now: u64, payload: Option<&[u8]>, token: &[u8]) -> TestResult<PreparedBytes> {
    frame_named(b"step", now, payload, token)
}

pub(super) fn frame_named(
    name: &[u8],
    now: u64,
    payload: Option<&[u8]>,
    token: &[u8],
) -> TestResult<PreparedBytes> {
    Ok(encode_step_continue(
        PreparedRequest {
            scope: common::scope()?,
            request: RequestId::new(name.to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: common::clock(now)?,
            operation: PreparedOperation::Batch(vec![values::item(
                b"i",
                now,
                Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(token.to_vec())?,
                        fence: Fence::new(1),
                    },
                    from: b"q".to_vec(),
                    to: b"charge".to_vec(),
                    worker: Some(b"w".to_vec()),
                    lease_ms: 30_000,
                    payload: payload.map(<[u8]>::to_vec),
                }),
            )?]),
        },
        limits().record_batch.codec,
        limits().record_batch.workspace_bytes,
    )?)
}

pub(super) fn apply(
    store: &mut RedbStateStore,
    frame: &PreparedBytes,
    index: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    let result = apply_step_continue_transaction(
        store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        limits(),
    );
    assert!(
        result.is_ok(),
        "healthy durable Step apply: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn retained(result: RecordBatchTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("expected retained result".into());
    };
    Ok(outcome)
}

pub(super) fn history(store: &RedbStateStore) -> TestResult<WorkflowHistoryPage> {
    Ok(store.read_workflow_history_page(
        &common::scope()?,
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
            maximum_row_reads: 10_000,
            maximum_inspected_bytes: 64 * common::MIB,
            maximum_returned_bytes: 64 * common::MIB,
            maximum_work: 64 * common::MIB,
            maximum_temporary_bytes: 64 * common::MIB,
        },
    )?)
}

pub(super) fn snapshot_limits() -> TestResult<value::HistorySnapshotLimits> {
    Ok(value::HistorySnapshotLimits::new(
        limits().record_batch.codec.admission(),
        common::MIB,
        common::MIB,
        4 * common::MIB,
    )?)
}
