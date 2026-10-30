use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::{Create, Operation};
use wf_ports::consensus::{
    PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_ports::log::encode_prepared;
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StoredWorkflowRecord, WorkflowHistoryDirection,
    WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};
use wf_storage::state::{StateStoreGenesis, StateStoreLimits};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::TimestampMillis;

pub(super) fn limits() -> WorkflowValueTransactionLimits {
    v::limits()
}

pub(super) fn fresh(path: &Path, stored: bool) -> TestResult<RedbStateStore> {
    let store = if stored {
        let scope = f::scope()?;
        let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
        RedbStateStore::create_stored_value_database(
            redb::Builder::new().create(path)?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0)?,
                placements: &placements,
                policies: &[],
            },
            v::defaults(),
            StateStoreLimits::default(),
        )?
    } else {
        v::fresh(path)??
    };
    Ok(store.upgrade_history_snapshot_layout()?)
}

pub(super) fn create(
    store: &mut RedbStateStore,
    id: &[u8],
    state: &[u8],
    time: (u64, u64, u64),
    payload: Option<&[u8]>,
) -> TestResult {
    let frame = v::batch(
        5,
        id,
        time.1,
        PreparedOperation::Batch(vec![v::item(
            id,
            time.1,
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: state.to_vec(),
                run_at: Some(TimestampMillis::new(time.2)),
                payload: payload.map(<[u8]>::to_vec),
            }),
        )?]),
        vec![v::input(0, 0)],
        vec![v::witness(b"r", None)?],
    )?;
    assert_eq!(
        retained_record(v::apply_value(store, &f::scope()?, &frame, time.0)?)?.status(),
        RetainedStatus::Accepted
    );
    Ok(())
}

pub(super) fn phase(
    request: &[u8],
    now: u64,
    state: &[u8],
    worker: &[u8],
    limit: usize,
) -> TestResult<PreparedBytes> {
    phase_with_intent(request, now, state, worker, limit, [42; 32])
}

pub(super) fn phase_with_intent(
    request: &[u8],
    now: u64,
    state: &[u8],
    worker: &[u8],
    limit: usize,
    intent: [u8; 32],
) -> TestResult<PreparedBytes> {
    Ok(encode_prepared(
        &PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(request.to_vec())?,
            intent: ClientIntent::new(1, intent)?,
            stamp: f::clock(now)?,
            operation: PreparedOperation::ClaimPhase(PreparedClaimPhase {
                query: PreparedClaimQuery {
                    workflow_type: b"r".to_vec(),
                    states: Some(vec![state.to_vec()]),
                    exclude_states: Vec::new(),
                    partitions: Some(vec![b"p".to_vec()]),
                    priority: None,
                    worker: worker.to_vec(),
                    lease_ms: 30000,
                    limit,
                },
                token_version: 1,
                first_ordinal: u32::MAX,
                scan_budget: 1000,
                head_budget: 1000,
            }),
        },
        limits().record_batch.codec,
    )?)
}

pub(super) fn apply(
    store: &mut RedbStateStore,
    frame: &PreparedBytes,
    at: u64,
) -> TestResult<ClaimTransactionOutcome> {
    let result = apply_workflow_value_claim_transaction(
        store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(at),
        limits(),
        ClaimWorkLimits::default(),
    );
    assert!(
        result.is_ok(),
        "healthy named value Claim: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn retained(result: ClaimTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("retained named Claim".into());
    };
    Ok(outcome)
}

pub(super) fn retained_record(
    result: wf_storage::transaction::RecordBatchTransactionOutcome,
) -> TestResult<Box<RetainedOutcome>> {
    let wf_storage::transaction::RecordBatchTransactionOutcome::Retained { outcome, .. } = result
    else {
        return Err("retained value mutation".into());
    };
    Ok(outcome)
}

pub(super) fn record(store: &mut RedbStateStore, id: &[u8]) -> TestResult<StoredWorkflowRecord> {
    let scope = f::scope()?;
    Ok(store
        .begin_apply(
            scope.group(),
            scope.recovery(),
            limits().record_batch.codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(id.to_vec())?,
        )?
        .ok_or("record")?)
}

pub(super) fn history(store: &RedbStateStore, id: &[u8]) -> TestResult<WorkflowHistoryPage> {
    Ok(store.read_workflow_history_page(
        &f::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
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

pub(super) fn cohort(path: &Path) -> TestResult<[BTreeMap<Vec<u8>, Vec<u8>>; 6]> {
    let db = redb::Builder::new().open(path)?;
    let read = db.begin_read()?;
    let mut result = std::array::from_fn(|_| BTreeMap::new());
    for (rows, name) in result.iter_mut().zip([
        "wf_state_meta_v1",
        "wf_state_rows_v1",
        "wf_state_snapshot_meta_v1",
        "wf_state_snapshot_rows_v1",
        "wf_state_snapshot_alt_meta_v1",
        "wf_state_snapshot_alt_rows_v1",
    ]) {
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
    }
    Ok(result)
}

pub(super) fn summary(outcome: &RetainedOutcome) -> TestResult<(RetainedStatus, Option<usize>)> {
    Ok(wf_ports::log::claim_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_time(),
        limits().record_batch.codec.admission(),
    )?)
}

pub(super) fn assert_first_id(outcome: &RetainedOutcome, id: &[u8]) -> TestResult {
    assert_eq!(summary(outcome)?, (RetainedStatus::Accepted, Some(1)));
    // FWCR header21, then literal raw partition p5, then the first raw record ID.
    let mut raw = u32::try_from(id.len())?.to_be_bytes().to_vec();
    raw.extend_from_slice(id);
    assert!(outcome.response()[26..].starts_with(&raw));
    Ok(())
}

pub(super) fn fifo(store: &mut RedbStateStore, at: u64, now: u64) -> TestResult {
    use wf_domain::policy::complete::RawFlowPolicyValue as Raw;
    let mode = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"charge"), Raw::Map(&mode))];
    let attrs = [(Raw::Atom("states"), Raw::Map(&states))];
    policy(store, at, now, Raw::Map(&attrs))
}

pub(super) fn policy(
    store: &mut RedbStateStore,
    at: u64,
    now: u64,
    attrs: wf_domain::policy::complete::RawFlowPolicyValue<'_>,
) -> TestResult {
    use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
    let mut charge = PolicySchemaCharge::new(limits().record_batch.schema);
    let frame = crate::original_policy::original_public_policy_frame(
        &f::scope()?,
        &RequestId::new(format!("policy-{at}").into_bytes())?,
        now,
        b"r",
        attrs,
        Raw::List(&[]),
        &mut charge,
    )?;
    assert_eq!(
        retained_record(
            wf_storage::transaction::apply_original_policy_set_transaction(
                store,
                &f::scope()?,
                &frame,
                f::log(at),
                limits().record_batch,
                &mut charge
            )?
        )?
        .status(),
        RetainedStatus::Accepted
    );
    Ok(())
}
