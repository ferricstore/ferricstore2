//! Actual public frames/stores and complete raw cohorts; no expected reducer.
use super::super::fixtures as old;
use super::TestResult;
use redb::{Durability, ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use std::{collections::BTreeMap, path::Path};
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::policy::complete::{
    CompletePolicyValue, PolicySchemaCharge, decode_complete_policy_value_v2,
};
use wf_domain::retention::ResolvedRetention;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedMetadata, RetainedStatus};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{PreparedBytes, encode_record_batch, prepare_record_batch};
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    ClaimTransactionLimits, ClaimTransactionOutcome, RecordBatchTransactionLimits,
    RecordBatchTransactionOutcome, apply_claim_transaction, apply_original_policy_set_transaction,
    apply_record_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};
mod frames;
mod wire;
pub(super) use frames::{complete_frame, frame_intent, public_frame};
pub(super) use old::{clock, directory, log, scope};
pub(super) use wire::{
    atom, binary, catalog, due, etf_map, etf_policy, finish, history_wire, key, manifest6, member,
    point, receipt, record_wire, rehash, snapshot_wire, stream3, upgrade6,
};
pub(super) type Cohort = [Option<BTreeMap<Vec<u8>, Vec<u8>>>; 6];
pub(super) const TABLES: [&str; 6] = [
    "wf_state_meta_v1",
    "wf_state_rows_v1",
    "wf_state_snapshot_meta_v1",
    "wf_state_snapshot_rows_v1",
    "wf_state_snapshot_alt_meta_v1",
    "wf_state_snapshot_alt_rows_v1",
];

pub(super) fn limits() -> RecordBatchTransactionLimits {
    let mut out = old::driver_limits();
    out.workspace_bytes = 1024 * old::MIB;
    out
}
pub(super) fn store_limits(history: u32) -> TestResult<StateStoreLimits> {
    Ok(StateStoreLimits::default()
        .with_history_max_events_limit(history)?
        .with_original_policy_reply_defaults(ResolvedRetention {
            ttl_ms: 604_800_000,
            history_max_events: history,
        })?)
}
pub(super) fn create(
    path: &Path,
    policies: &[(Vec<u8>, Vec<u8>)],
    history: u32,
) -> TestResult<RedbStateStore> {
    let namespace = scope()?;
    let profile = limits().schema;
    let policies = policies
        .iter()
        .map(|(kind, raw)| {
            let value = decode_complete_policy_value_v2(
                raw,
                profile,
                &mut PolicySchemaCharge::new(profile),
            )?;
            assert_eq!(value.data().workflow_type, *kind);
            Ok((namespace.tenant().clone(), value))
        })
        .collect::<TestResult<Vec<(wf_types::identity::TenantId, CompletePolicyValue)>>>()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, namespace.group().clone())];
    Ok(RedbStateStore::create(
        path,
        namespace.group().clone(),
        namespace.recovery().clone(),
        StateStoreGenesis {
            initial_time: clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        store_limits(history)?,
    )?)
}
pub(super) fn open(path: &Path, history: u32) -> TestResult<RedbStateStore> {
    let namespace = scope()?;
    Ok(RedbStateStore::open(
        path,
        namespace.group().clone(),
        namespace.recovery().clone(),
        store_limits(history)?,
    )?)
}
pub(super) fn cohort(path: &Path) -> TestResult<Cohort> {
    let db = redb::Builder::new().open(path)?;
    let read = db.begin_read()?;
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let names = read
        .list_tables()?
        .map(|t| t.name().to_owned())
        .collect::<Vec<_>>();
    assert!(names.iter().all(|name| TABLES.contains(&name.as_str())));
    assert!(
        TABLES[..4]
            .iter()
            .all(|name| names.iter().any(|actual| actual == name))
    );
    assert_eq!(
        names.iter().any(|name| name == TABLES[4]),
        names.iter().any(|name| name == TABLES[5])
    );
    let mut result = std::array::from_fn(|_| None);
    for (slot, name) in result.iter_mut().zip(TABLES) {
        if !names.iter().any(|actual| actual == name) {
            continue;
        }
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
        *slot = Some(rows);
    }
    Ok(result)
}

pub(super) fn rows(cohort: &Cohort) -> TestResult<&BTreeMap<Vec<u8>, Vec<u8>>> {
    Ok(cohort[1].as_ref().ok_or("missing live rows")?)
}

pub(super) fn rows_mut(cohort: &mut Cohort) -> TestResult<&mut BTreeMap<Vec<u8>, Vec<u8>>> {
    Ok(cohort[1].as_mut().ok_or("missing live rows")?)
}

pub(super) fn batch(
    namespace: &RequestScope,
    request: &[u8],
    now: u64,
    items: Vec<PreparedBatchItem>,
) -> TestResult<PreparedBytes> {
    let input = PreparedRequest {
        scope: namespace.clone(),
        request: RequestId::new(request.to_vec())?,
        intent: wf_domain::workflow::request::ClientIntent::new(1, [0; 32])?,
        stamp: clock(now)?,
        operation: PreparedOperation::Batch(items),
    };
    Ok(encode_record_batch(
        input,
        limits().codec,
        limits().workspace_bytes,
    )?)
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
    namespace: &RequestScope,
    partition: &[u8],
    data: WorkflowRecordData,
    priority: u8,
    sequence: u128,
) -> TestResult<StoredWorkflowRecord> {
    Ok(StoredWorkflowRecord {
        tenant: namespace.tenant().clone(),
        partition: PartitionId::new(partition.to_vec())?,
        record: restore_record(data, AdmissionLimits::default())?,
        ordering: OrderingMetadata {
            priority,
            state_enter_seq: sequence,
        },
    })
}

pub(super) fn seed(
    store: &mut RedbStateStore,
    namespace: &RequestScope,
    owners: &[StoredWorkflowRecord],
    at: u64,
    now: u64,
) -> TestResult {
    let items = owners
        .iter()
        .map(|row| PreparedBatchItem {
            partition: row.partition.clone(),
            create_priority: row.ordering.priority,
            command: CommittedCommand {
                id: row.record.id().clone(),
                now: TimestampMillis::new(now),
                operation: Operation::Create(Create {
                    workflow_type: row.record.workflow_type().to_vec(),
                    state: row.record.state().to_vec(),
                    run_at: row.record.next_run_at(),
                    payload: row.record.payload().map(<[u8]>::to_vec),
                }),
            },
        })
        .collect();
    let bytes = batch(namespace, format!("seed-{at}").as_bytes(), now, items)?;
    assert!(
        matches!(apply_record_batch_transaction(store, namespace, bytes.as_bytes(), log(at), limits())?,
        RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.status() == RetainedStatus::Accepted)
    );
    Ok(())
}

fn filter(bytes: &mut Vec<u8>, values: Option<&[&[u8]]>) -> TestResult {
    bytes.push(u8::from(values.is_some()));
    if let Some(values) = values {
        bytes.extend_from_slice(&u32::try_from(values.len())?.to_be_bytes());
        for value in values {
            raw(bytes, value)?;
        }
    }
    Ok(())
}

pub(super) fn claim_frame(
    namespace: &RequestScope,
    request: &[u8],
    now: u64,
    kind: &[u8],
    worker: &[u8],
    states: Option<&[&[u8]]>,
    partitions: Option<&[&[u8]]>,
) -> TestResult<Vec<u8>> {
    let mut bytes = b"FWCP\0\x01\x03".to_vec();
    for field in [
        namespace.tenant().as_bytes(),
        namespace.group().as_bytes(),
        namespace.recovery().as_bytes(),
        request,
    ] {
        raw(&mut bytes, field)?;
    }
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&[61; 32]);
    bytes.extend_from_slice(&now.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&now.to_be_bytes());
    raw(&mut bytes, kind)?;
    filter(&mut bytes, states)?;
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    filter(&mut bytes, partitions)?;
    bytes.push(0); // Any priority.
    raw(&mut bytes, worker)?;
    bytes.extend_from_slice(&30_000_u64.to_be_bytes());
    for number in [1_u32, 16_384, 4_096] {
        bytes.extend_from_slice(&number.to_be_bytes());
    }
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    Ok(bytes)
}

pub(super) fn claim(
    store: &mut RedbStateStore,
    namespace: &RequestScope,
    frame: &[u8],
    at: u64,
    response: &[u8],
) -> TestResult {
    let profile = ClaimTransactionLimits {
        base: limits(),
        work: ClaimWorkLimits::default(),
    };
    assert!(
        matches!(apply_claim_transaction(store, namespace, frame, log(at), profile)?,
        ClaimTransactionOutcome::Retained { outcome, .. } if outcome.response() == response)
    );
    Ok(())
}

pub(super) fn raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

pub(super) fn optional(out: &mut Vec<u8>, value: Option<&[u8]>) -> TestResult {
    out.push(u8::from(value.is_some()));
    if let Some(value) = value {
        raw(out, value)?;
    }
    Ok(())
}

pub(super) fn metadata(
    namespace: &RequestScope,
    request: &[u8],
    at: u64,
    now: u64,
    intent: ClientIntent,
    status: RetainedStatus,
) -> TestResult<RetainedMetadata> {
    Ok(RetainedMetadata {
        scope: namespace.clone(),
        request: RequestId::new(request.to_vec())?,
        intent,
        original_position: log(at),
        original_time: TimestampMillis::new(now),
        expires_at: TimestampMillis::new(now + 1_000),
        status,
    })
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

pub(super) fn put_record(
    cohort: &mut Cohort,
    namespace: &RequestScope,
    owner: &StoredWorkflowRecord,
) -> TestResult {
    let rows = rows_mut(cohort)?;
    rows.insert(point(namespace, owner), record_wire(namespace, owner)?);
    let (key, value) = history_wire(namespace, owner, &history(owner))?;
    rows.insert(key, value);
    Ok(())
}

pub(super) fn batch_intent(frame: &[u8], at: u64) -> TestResult<ClientIntent> {
    Ok(
        prepare_record_batch(frame, limits().codec, limits().workspace_bytes)?
            .header(log(at))
            .intent,
    )
}

pub(super) fn replace(path: &Path, cohort: &Cohort) -> TestResult {
    let db = redb::Builder::new().open(path)?;
    let mut tx = db.begin_write()?;
    tx.set_durability(Durability::Immediate)?;
    for (name, rows) in TABLES.into_iter().zip(cohort) {
        let definition = TableDefinition::<&[u8], &[u8]>::new(name);
        if let Some(rows) = rows {
            let mut table = tx.open_table(definition)?;
            table.retain(|_, _| false)?;
            for (key, value) in rows {
                table.insert(key.as_slice(), value.as_slice())?;
            }
        } else {
            tx.delete_table(definition)?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn apply(
    store: &mut RedbStateStore,
    frame: &[u8],
    at: u64,
    profile: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<RecordBatchTransactionOutcome> {
    Ok(apply_original_policy_set_transaction(
        store,
        &scope()?,
        frame,
        log(at),
        profile,
        charge,
    )?)
}
pub(super) fn seed_records(
    store: &mut RedbStateStore,
    commands: &[PreparedBatchItem],
    at: u64,
) -> TestResult {
    let frame = batch(
        &scope()?,
        format!("seed-{at}").as_bytes(),
        0,
        commands.to_vec(),
    )?;
    assert!(
        matches!(apply_record_batch_transaction(store,&scope()?,frame.as_bytes(),log(at),limits())?,
  RecordBatchTransactionOutcome::Retained{outcome,..}if outcome.status()==RetainedStatus::Accepted)
    );
    Ok(())
}
