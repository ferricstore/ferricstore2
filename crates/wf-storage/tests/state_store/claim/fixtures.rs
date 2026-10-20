//! Mechanical input/oracle packing, without a second workflow reducer or parser.

use super::super::fixtures as old;
use super::TestResult;
use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::policy::complete::{
    CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_ports::consensus::{
    PreparedBatchItem, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_ports::log::encode_record_batch;
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    ClaimTransactionLimits, ClaimTransactionOutcome, RecordBatchTransactionOutcome,
    apply_claim_transaction, apply_record_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{CommittedTime, Fence, LogIdentity, TimestampMillis};

mod wire;
pub(super) use wire::{history_wire, outcome_wire, receipt, record_wire};

pub(super) use old::{clock, directory, log, scope};
/// Complete raw owners in the fixed four-table order below.
pub(super) type Cohort = [BTreeMap<Vec<u8>, Vec<u8>>; 4];
pub(super) const TABLES: [&str; 4] = [
    "wf_state_meta_v1",
    "wf_state_rows_v1",
    "wf_state_snapshot_meta_v1",
    "wf_state_snapshot_rows_v1",
];

pub(super) fn limits() -> ClaimTransactionLimits {
    ClaimTransactionLimits {
        base: old::driver_limits(),
        work: ClaimWorkLimits::default(),
    }
}

pub(super) fn raw(output: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    output.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn optional(output: &mut Vec<u8>, bytes: Option<&[u8]>) -> TestResult {
    output.push(u8::from(bytes.is_some()));
    if let Some(bytes) = bytes {
        raw(output, bytes)?;
    }
    Ok(())
}

fn filter(output: &mut Vec<u8>, values: Option<&[Vec<u8>]>) -> TestResult {
    output.push(u8::from(values.is_some()));
    if let Some(values) = values {
        output.extend_from_slice(&u32::try_from(values.len())?.to_be_bytes());
        for value in values {
            raw(output, value)?;
        }
    }
    Ok(())
}

pub(super) fn query(kind: &[u8], worker: &[u8], limit: usize) -> PreparedClaimQuery {
    PreparedClaimQuery {
        workflow_type: kind.to_vec(),
        states: None,
        exclude_states: Vec::new(),
        partitions: None,
        priority: None,
        worker: worker.to_vec(),
        lease_ms: 30_000,
        limit,
    }
}

pub(super) fn phase(
    scope: &RequestScope,
    request: &[u8],
    now: u64,
    query: &PreparedClaimQuery,
    budgets: (usize, usize),
    intent: [u8; 32],
) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWCP\0\x01\x03".to_vec();
    for value in [
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request,
    ] {
        raw(&mut bytes, value)?;
    }
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&intent);
    bytes.extend_from_slice(&now.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&now.to_be_bytes());
    raw(&mut bytes, &query.workflow_type)?;
    filter(&mut bytes, query.states.as_deref())?;
    bytes.extend_from_slice(&u32::try_from(query.exclude_states.len())?.to_be_bytes());
    for value in &query.exclude_states {
        raw(&mut bytes, value)?;
    }
    filter(&mut bytes, query.partitions.as_deref())?;
    bytes.push(u8::from(query.priority.is_some()));
    if let Some(priority) = query.priority {
        bytes.push(priority);
    }
    raw(&mut bytes, &query.worker)?;
    bytes.extend_from_slice(&query.lease_ms.to_be_bytes());
    for value in [query.limit, budgets.0, budgets.1] {
        bytes.extend_from_slice(&u32::try_from(value)?.to_be_bytes());
    }
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    Ok(bytes)
}

pub(super) fn policy(
    kind: &[u8],
    states: &[&[u8]],
    governance: bool,
) -> TestResult<CompletePolicyValue> {
    let mut snapshot = b"FWPS\0\x02".to_vec();
    raw(&mut snapshot, kind)?;
    snapshot.extend_from_slice(&[0; 4]);
    snapshot.extend_from_slice(&u32::try_from(states.len())?.to_be_bytes());
    for state in states {
        raw(&mut snapshot, state)?;
        snapshot.extend_from_slice(&[1, 1, 0, 0, 0]);
    }
    snapshot.extend_from_slice(&[0, 0, u8::from(governance)]);
    if governance {
        snapshot.extend_from_slice(&[0; 7]);
    }
    let mut value = b"FWPV\0\x02".to_vec();
    value.extend_from_slice(&1_u64.to_be_bytes());
    raw(&mut value, &snapshot)?;
    let schema = PolicySchemaLimits::default();
    Ok(decode_complete_policy_value_v2(
        &value,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?)
}

pub(super) fn create(
    path: &Path,
    scope: &RequestScope,
    partitions: &[&[u8]],
    policy: Option<CompletePolicyValue>,
) -> TestResult<RedbStateStore> {
    let placements = partitions
        .iter()
        .map(|p| Ok((PartitionId::new(p.to_vec())?, scope.group().clone())))
        .collect::<TestResult<Vec<_>>>()?;
    let policies = policy
        .into_iter()
        .map(|policy| (scope.tenant().clone(), policy))
        .collect::<Vec<_>>();
    Ok(RedbStateStore::create(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?)
}

pub(super) fn open(path: &Path, scope: &RequestScope) -> TestResult<RedbStateStore> {
    Ok(RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?)
}

pub(super) fn replace_rows(path: &Path, rows: &BTreeMap<Vec<u8>, Vec<u8>>) -> TestResult {
    old::replace(path, rows, &old::raw_manifest(path)?)
}

pub(super) fn metadata(
    scope: &RequestScope,
    request: &[u8],
    at: u64,
    now: u64,
    digest: [u8; 32],
    status: RetainedStatus,
) -> TestResult<RetainedMetadata> {
    Ok(RetainedMetadata {
        scope: scope.clone(),
        request: RequestId::new(request.to_vec())?,
        intent: ClientIntent::new(1, digest)?,
        original_position: log(at),
        original_time: TimestampMillis::new(now),
        expires_at: TimestampMillis::new(now + 1_000),
        status,
    })
}

pub(super) fn data(
    id: &[u8],
    kind: &[u8],
    state: &[u8],
    now: u64,
    due: Option<u64>,
) -> TestResult<WorkflowRecordData> {
    Ok(WorkflowRecordData {
        id: RecordId::new(id.to_vec())?,
        workflow_type: kind.to_vec(),
        state: state.to_vec(),
        version: 1,
        fence: Fence::new(0),
        attempts: 0,
        created_at: TimestampMillis::new(now),
        updated_at: TimestampMillis::new(now),
        next_run_at: due.map(TimestampMillis::new),
        run_state: None,
        lease: None,
        payload: None,
        result: None,
        error: None,
        latest_kind: EventKind::Created,
        latest_version: 1,
        latest_fence: Fence::new(0),
    })
}

pub(super) fn owner(
    scope: &RequestScope,
    partition: &[u8],
    data: WorkflowRecordData,
    ordering: OrderingMetadata,
) -> TestResult<StoredWorkflowRecord> {
    Ok(StoredWorkflowRecord {
        tenant: scope.tenant().clone(),
        partition: PartitionId::new(partition.to_vec())?,
        record: restore_record(data, AdmissionLimits::default())?,
        ordering,
    })
}

pub(super) fn seed(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    owners: &[StoredWorkflowRecord],
    at: u64,
    now: u64,
) -> TestResult {
    let items = owners
        .iter()
        .map(|owner| PreparedBatchItem {
            partition: owner.partition.clone(),
            create_priority: owner.ordering.priority,
            command: CommittedCommand {
                id: owner.record.id().clone(),
                now: TimestampMillis::new(now),
                operation: Operation::Create(Create {
                    workflow_type: owner.record.workflow_type().to_vec(),
                    state: owner.record.state().to_vec(),
                    run_at: owner.record.next_run_at(),
                    payload: owner.record.payload().map(<[u8]>::to_vec),
                }),
            },
        })
        .collect();
    let request = PreparedRequest {
        scope: scope.clone(),
        request: RequestId::new(format!("seed-{at}").into_bytes())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(now)?,
        operation: PreparedOperation::Batch(items),
    };
    let profile = old::driver_limits();
    let frame = encode_record_batch(request, profile.codec, profile.workspace_bytes)?;
    assert!(matches!(
        apply_record_batch_transaction(store, scope, frame.as_bytes(), log(at), profile)?,
        RecordBatchTransactionOutcome::Retained { outcome, .. }
            if outcome.status() == RetainedStatus::Accepted
    ));
    Ok(())
}

pub(super) fn cohort(path: &Path) -> TestResult<Cohort> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let mut result = std::array::from_fn(|_| BTreeMap::new());
    for (slot, name) in result.iter_mut().zip(TABLES) {
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                slot.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
    }
    Ok(result)
}

pub(super) fn key(family: u8, fields: &[&[u8]]) -> Vec<u8> {
    let mut bytes = b"FWPK\0\x01".to_vec();
    bytes.push(family);
    for field in fields {
        for byte in *field {
            bytes.push(*byte);
            if *byte == 0 {
                bytes.push(255);
            }
        }
        bytes.extend_from_slice(&[0, 0]);
    }
    bytes
}

pub(super) fn point(scope: &RequestScope, owner: &StoredWorkflowRecord) -> Vec<u8> {
    key(
        1,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            owner.partition.as_bytes(),
            owner.record.id().as_bytes(),
        ],
    )
}

pub(super) fn due(scope: &RequestScope, owner: &StoredWorkflowRecord) -> TestResult<Vec<u8>> {
    let row = &owner.record;
    let mut bytes = key(
        3,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
    );
    bytes.push(2 - owner.ordering.priority);
    bytes.extend_from_slice(
        &row.next_run_at()
            .ok_or("oracle due absent")?
            .get()
            .to_be_bytes(),
    );
    let tail = key(
        3,
        &[
            row.id().as_bytes(),
            owner.partition.as_bytes(),
            row.run_state().unwrap_or(row.state()),
            row.workflow_type(),
        ],
    );
    bytes.extend_from_slice(&tail[7..]);
    Ok(bytes)
}

pub(super) fn member(scope: &RequestScope, owner: &StoredWorkflowRecord) -> Vec<u8> {
    let row = &owner.record;
    let mut bytes = key(
        4,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            row.workflow_type(),
            row.run_state().unwrap_or(row.state()),
            owner.partition.as_bytes(),
        ],
    );
    bytes.push(u8::from(row.state() != b"running"));
    bytes.extend_from_slice(&owner.ordering.state_enter_seq.to_be_bytes());
    bytes.extend_from_slice(&key(4, &[row.id().as_bytes()])[7..]);
    bytes
}

pub(super) fn history(owner: &StoredWorkflowRecord) -> HistoryEntry {
    let row = &owner.record;
    HistoryEntry {
        event_id: format!("{}-{}", row.updated_at().get(), row.version()),
        kind: row.latest_event_witness().0,
        occurred_at: row.updated_at(),
        version: row.version(),
        fence: row.fence(),
        attempts: row.attempts(),
        state: row.state().to_vec(),
        next_run_at: row.next_run_at(),
        lease_deadline: row.lease_deadline(),
        worker: row.lease().map(|lease| lease.worker.clone()),
        retry: None,
    }
}

fn put_time(bytes: &mut Vec<u8>, time: CommittedTime) {
    bytes.extend_from_slice(&time.hlc().physical().get().to_be_bytes());
    bytes.extend_from_slice(&time.hlc().logical().to_be_bytes());
    bytes.extend_from_slice(&time.wall().get().to_be_bytes());
}

pub(super) fn finish(
    rows: &mut [BTreeMap<Vec<u8>, Vec<u8>>; 4],
    scope: &RequestScope,
    frame: &[u8],
    at: LogIdentity,
    times: (CommittedTime, CommittedTime),
    new_outcome: Option<(&RetainedMetadata, &[u8])>,
) -> TestResult {
    let mut control = b"FWSC\0\x01".to_vec();
    raw(&mut control, scope.group().as_bytes())?;
    raw(&mut control, scope.recovery().as_bytes())?;
    put_time(&mut control, times.0);
    control.push(1);
    for number in [at.term(), at.leader_node(), at.index().get()] {
        control.extend_from_slice(&number.to_be_bytes());
    }
    control.extend_from_slice(&Sha256::digest(frame));
    control.push(1);
    put_time(&mut control, times.1);
    rows[1].insert(
        key(0, &[scope.group().as_bytes(), scope.recovery().as_bytes()]),
        control,
    );
    if let Some((metadata, response)) = new_outcome {
        rows[1].insert(
            key(
                2,
                &[
                    scope.group().as_bytes(),
                    scope.recovery().as_bytes(),
                    scope.tenant().as_bytes(),
                    metadata.request.as_bytes(),
                ],
            ),
            outcome_wire(metadata, response)?,
        );
    }
    Ok(())
}

pub(super) fn replace_expected(
    rows: &mut [BTreeMap<Vec<u8>, Vec<u8>>; 4],
    scope: &RequestScope,
    after: &StoredWorkflowRecord,
    history: &HistoryEntry,
    deletes: &[Vec<u8>],
    inserts: &[(Vec<u8>, Vec<u8>)],
) -> TestResult {
    rows[1].insert(point(scope, after), record_wire(scope, after)?);
    let (key, wire) = history_wire(scope, after, history)?;
    rows[1].insert(key, wire);
    for key in deletes {
        assert!(rows[1].remove(key).is_some());
    }
    for (key, value) in inserts {
        rows[1].insert(key.clone(), value.clone());
    }
    Ok(())
}

pub(super) fn apply_check(
    path: &Path,
    scope: &RequestScope,
    frame: &[u8],
    at: LogIdentity,
    limits: ClaimTransactionLimits,
    expected: &[BTreeMap<Vec<u8>, Vec<u8>>; 4],
    response: &[u8],
) -> TestResult<Box<RetainedOutcome>> {
    let mut store = open(path, scope)?;
    let ClaimTransactionOutcome::Retained { control, outcome } =
        apply_claim_transaction(&mut store, scope, frame, at, limits)?
    else {
        return Err("Claim did not return retained acknowledgement".into());
    };
    assert_eq!(control.position(), at.index());
    assert_eq!(outcome.response_version(), 3);
    assert_eq!(outcome.response(), response);
    drop(store);
    assert_eq!(cohort(path)?, *expected);
    drop(open(path, scope)?);
    Ok(outcome)
}
