//! Shared test donors and raw disk observations, with no production admin API.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use redb::{Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use sha2::{Digest, Sha256};
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder};
use wf_domain::history::EventKind;
use wf_domain::policy::complete::{
    CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{Cancel, CommittedCommand, Create, Operation, Outcome, reduce};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecLimits, PreparedBytes, encode_independent_record_batch, encode_record_batch,
    prepare_independent_record_batch, prepare_public_mutation, prepare_record_batch,
};
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits, key, value};
use wf_storage::transaction::RecordBatchTransactionLimits;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

use super::TestResult;

pub(super) const MIB: usize = 1_048_576;
pub(super) const META: TableDefinition<'static, &[u8], &[u8]> =
    TableDefinition::new("wf_state_meta_v1");
pub(super) const ROWS: TableDefinition<'static, &[u8], &[u8]> =
    TableDefinition::new("wf_state_rows_v1");
static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
const RICH: &str = "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000";

pub(super) fn directory(tag: &str) -> TestResult<PathBuf> {
    let suffix = NEXT_PATH.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "wf-state-store-{}-{suffix}-{tag}",
        std::process::id()
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

pub(super) fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

pub(super) fn genesis_control_lower_bound(path: &Path) -> TestResult {
    let scope = scope()?;
    let original = raw_rows(path)?;
    let original_manifest = raw_manifest(path)?;
    let mut manifest = original_manifest.clone();
    assert_eq!(manifest.len(), 110);
    // FWSS2 with one-byte group/recovery: time starts at12, checksum at78.
    manifest[20..22].copy_from_slice(&1_u16.to_be_bytes());
    let checksum = Sha256::digest(&manifest[..78]);
    manifest[78..].copy_from_slice(&checksum);
    let control_key = key::encode_control_key(scope.group(), scope.recovery(), MIB)?;
    let identity = EntryIdentity::new(log(0), PreparedDigest::new([0x5a; 32]));
    let stamp = EntryStamp::Stamped(clock(16)?);
    let mut rows = original.clone();
    for (physical, logical, wall, valid) in [
        (17, 1, 17, true),
        (17, 0, 17, false),
        (17, 1, 16, false),
        (16, 0, 16, false),
    ] {
        let time = CommittedTime::new(HlcTimestamp::new(physical, logical)?, wall)?;
        let control = ControlState::restored(time, identity, stamp)?;
        rows.insert(
            control_key.clone(),
            value::encode_control(scope.group(), scope.recovery(), control, MIB)?,
        );
        replace(path, &rows, &manifest)?;
        let reopened = open(path);
        if valid {
            drop(reopened?);
        } else {
            assert!(
                matches!(reopened, Err(error)
                if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt)),
                "control HLC({physical},{logical})/wall{wall} precedes genesis(17,1)/17"
            );
        }
        assert_eq!(raw_rows(path)?, rows);
        assert_eq!(raw_manifest(path)?, manifest);
    }
    replace(path, &original, &original_manifest)
}

pub(super) fn missing_genesis_control(path: &Path) -> TestResult {
    let scope = scope()?;
    let mut rows = raw_rows(path)?;
    let manifest = raw_manifest(path)?;
    assert!(
        rows.remove(&key::encode_control_key(
            scope.group(),
            scope.recovery(),
            MIB
        )?)
        .is_some()
    );
    replace(path, &rows, &manifest)?;
    assert!(matches!(open(path), Err(error)
        if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt)));
    assert_eq!(raw_rows(path)?, rows);
    assert_eq!(raw_manifest(path)?, manifest);
    Ok(())
}

pub(super) fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn driver_limits() -> RecordBatchTransactionLimits {
    RecordBatchTransactionLimits {
        codec: CodecLimits::new(AdmissionLimits::default()),
        schema: PolicySchemaLimits::default(),
        maximum_read_steps: 10_000,
        maximum_read_bytes: 64 * MIB,
        maximum_staging_bytes: 64 * MIB,
        maximum_adapter_bytes: 32 * MIB,
        workspace_bytes: 256 * MIB,
        outcome_ttl_ms: 1_000,
    }
}

pub(super) fn placements() -> TestResult<Vec<(PartitionId, GroupId)>> {
    Ok(vec![(
        PartitionId::new(b"p".to_vec())?,
        GroupId::new(b"g".to_vec())?,
    )])
}

pub(super) fn rich_policy() -> TestResult<CompletePolicyValue> {
    let wire = RICH
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect::<TestResult<Vec<_>>>()?;
    let limits = PolicySchemaLimits::default();
    Ok(decode_complete_policy_value_v2(
        &wire,
        limits,
        &mut PolicySchemaCharge::new(limits),
    )?)
}

pub(super) fn create(path: &Path) -> TestResult<RedbStateStore> {
    let scope = scope()?;
    let placement = placements()?;
    let policies = [(scope.tenant().clone(), rich_policy()?)];
    Ok(RedbStateStore::create(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: clock(0)?,
            placements: &placement,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?)
}

pub(super) fn open(path: &Path) -> TestResult<RedbStateStore> {
    let scope = scope()?;
    Ok(RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?)
}

pub(super) fn raw_rows(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(ROWS)?;
    table
        .iter()?
        .map(|row| {
            let (key, value) = row?;
            Ok((key.value().to_vec(), value.value().to_vec()))
        })
        .collect()
}

pub(super) fn raw_manifest(path: &Path) -> TestResult<Vec<u8>> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(META)?;
    assert_eq!(table.len()?, 2);
    assert!(table.get(b"membership".as_slice())?.is_some());
    let guard = table
        .get(b"manifest".as_slice())?
        .ok_or("manifest absent")?;
    Ok(guard.value().to_vec())
}

pub(super) fn replace(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    manifest: &[u8],
) -> TestResult {
    let database = redb::Builder::new().create(path)?;
    let mut write = database.begin_write()?;
    write.set_durability(Durability::Immediate)?;
    {
        let mut table = write.open_table(ROWS)?;
        table.retain(|_, _| false)?;
        for (key, value) in rows {
            table.insert(key.as_slice(), value.as_slice())?;
        }
        let mut metadata = write.open_table(META)?;
        let membership = metadata
            .get(b"membership".as_slice())?
            .ok_or("membership absent")?
            .value()
            .to_vec();
        metadata.retain(|_, _| false)?;
        metadata.insert(b"manifest".as_slice(), manifest)?;
        metadata.insert(b"membership".as_slice(), membership.as_slice())?;
    }
    write.commit()?;
    Ok(())
}

pub(super) fn create_op() -> Operation {
    Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(vec![0, 255]),
    })
}

pub(super) fn cancel_op(version: u64) -> Operation {
    Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: Some(b"q".to_vec()),
        expected_version: Some(version),
        error: Some(b"cancelled".to_vec()),
    })
}

pub(super) fn frame(
    request_id: &[u8],
    now: u64,
    entries: Vec<(&[u8], Operation)>,
) -> TestResult<PreparedBytes> {
    Ok(encode_record_batch(
        request(request_id, now, entries)?,
        driver_limits().codec,
        driver_limits().workspace_bytes,
    )?)
}

pub(super) fn independent_frame(
    request_id: &[u8],
    now: u64,
    entries: Vec<(&[u8], Operation)>,
) -> TestResult<PreparedBytes> {
    Ok(encode_independent_record_batch(
        request(request_id, now, entries)?,
        driver_limits().codec,
        driver_limits().workspace_bytes,
    )?)
}

fn request(
    request_id: &[u8],
    now: u64,
    entries: Vec<(&[u8], Operation)>,
) -> TestResult<PreparedRequest> {
    let items = entries
        .into_iter()
        .map(|(id, operation)| {
            Ok(PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(id.to_vec())?,
                    now: TimestampMillis::new(now),
                    operation,
                },
                create_priority: 0,
            })
        })
        .collect::<TestResult<Vec<_>>>()?;
    Ok(PreparedRequest {
        scope: scope()?,
        request: RequestId::new(request_id.to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(now)?,
        operation: PreparedOperation::Batch(items),
    })
}

/// Complete test rebuild from canonical records and full installed policies.
pub(super) fn reindex(rows: &mut BTreeMap<Vec<u8>, Vec<u8>>) -> TestResult {
    let scope = scope()?;
    let owners = rows
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&1))
        .map(|(_, wire)| {
            Ok(value::decode_record(
                wire,
                scope.group(),
                scope.recovery(),
                AdmissionLimits::default(),
                16 * MIB,
            )?)
        })
        .collect::<TestResult<Vec<_>>>()?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let policies = rows
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&6))
        .map(|(_, wire)| Ok(decode_complete_policy_value_v2(wire, schema, &mut charge)?))
        .collect::<TestResult<Vec<_>>>()?;
    rows.retain(|key, _| !matches!(key.get(6), Some(3 | 4)));
    let mut heads = BTreeMap::new();
    for owner in &owners {
        let entry = ClaimRecord {
            record: &owner.record,
            partition_key: owner.partition.as_bytes(),
            priority: owner.ordering.priority,
            state_enter_seq: owner.ordering.state_enter_seq,
        };
        let Some(lane) = LaneKey::for_record(&entry) else {
            continue;
        };
        let mode = policies
            .iter()
            .find(|p| p.data().workflow_type == owner.record.workflow_type())
            .and_then(|p| p.data().states.get(lane.state))
            .and_then(|state| state.mode)
            .unwrap_or(StateMode::Parallel);
        if mode == StateMode::Fifo {
            let member = LaneOrder::for_record(&entry);
            rows.insert(key::encode_fifo_key(&scope, lane, member, MIB)?, Vec::new());
            let head = heads.entry(lane).or_insert((member, entry));
            if member < head.0 {
                *head = (member, entry);
            }
        } else if let Some(due) = DueOrder::for_record(&entry) {
            rows.insert(key::encode_due_key(&scope, due, MIB)?, vec![0]);
        }
    }
    for (_, (_, entry)) in heads {
        if let Some(due) = DueOrder::for_record(&entry) {
            rows.insert(key::encode_due_key(&scope, due, MIB)?, vec![1]);
        }
    }
    Ok(())
}

/// Expected writes use the original one-record reducer, not the new publisher.
pub(super) fn step(
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    id: &[u8],
    now: u64,
    operation: Operation,
    sequence: u128,
    version: u64,
) -> TestResult {
    let scope = scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(id.to_vec())?;
    let point = key::encode_record_key(&scope, &partition, &id, MIB)?;
    let previous = rows
        .get(&point)
        .map(|wire| {
            value::decode_record(
                wire,
                scope.group(),
                scope.recovery(),
                AdmissionLimits::default(),
                16 * MIB,
            )
        })
        .transpose()?;
    let plan = reduce(
        previous.as_ref().map(|owner| &owner.record),
        &CommittedCommand {
            id,
            now: TimestampMillis::new(now),
            operation,
        },
    )?;
    assert_eq!(plan.outcome(), Outcome::Changed);
    let (_, write, history) = plan.into_parts();
    let write = write.ok_or("expected write absent")?;
    let history = history.ok_or("expected history absent")?;
    assert_eq!(
        write.expected_version,
        previous.as_ref().map(|owner| owner.record.version())
    );
    assert_eq!(write.record.version(), version);
    assert_eq!(history.version, version);
    let owner = StoredWorkflowRecord {
        tenant: scope.tenant().clone(),
        partition,
        record: write.record,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: sequence,
        },
    };
    rows.insert(
        point,
        value::encode_record(
            scope.group(),
            scope.recovery(),
            &owner,
            AdmissionLimits::default(),
            16 * MIB,
        )?,
    );
    rows.insert(
        key::encode_history_key(&scope, &owner.partition, owner.record.id(), &history, MIB)?,
        value::encode_history(
            &scope,
            &owner.partition,
            owner.record.id(),
            &history,
            16 * MIB,
        )?,
    );
    Ok(())
}

pub(super) fn control_only(
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    encoded: &[u8],
    at: LogIdentity,
    profile: u8,
) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let scope = scope()?;
    let limits = driver_limits();
    let decoded = match profile {
        5 => prepare_record_batch(encoded, limits.codec, limits.workspace_bytes)?,
        6 => prepare_independent_record_batch(encoded, limits.codec, limits.workspace_bytes)?,
        1 => prepare_public_mutation(encoded, limits.codec, limits.workspace_bytes)?,
        _ => return Err("unsupported test profile".into()),
    };
    let header = decoded.header(at);
    let control_key = key::encode_control_key(scope.group(), scope.recovery(), MIB)?;
    let old = value::decode_control(
        rows.get(&control_key).ok_or("old control absent")?,
        scope.group(),
        scope.recovery(),
        MIB,
    )?;
    let control = plan_control(old, header.identity, EntryStamp::Stamped(header.stamp))?;
    let mut expected = rows.clone();
    expected.insert(
        control_key,
        value::encode_control(scope.group(), scope.recovery(), control.next(), MIB)?,
    );
    Ok(expected)
}

pub(super) fn finish(
    mut rows: BTreeMap<Vec<u8>, Vec<u8>>,
    encoded: &[u8],
    at: LogIdentity,
    profile: u8,
    body: Vec<u8>,
    status: RetainedStatus,
) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    reindex(&mut rows)?;
    let expected = control_only(&rows, encoded, at, profile)?;
    let scope = scope()?;
    let limits = driver_limits();
    let decoded = match profile {
        5 => prepare_record_batch(encoded, limits.codec, limits.workspace_bytes)?,
        6 => prepare_independent_record_batch(encoded, limits.codec, limits.workspace_bytes)?,
        1 => prepare_public_mutation(encoded, limits.codec, limits.workspace_bytes)?,
        _ => return Err("unsupported test profile".into()),
    };
    let header = decoded.header(at);
    let old = value::decode_control(
        rows.get(&key::encode_control_key(
            scope.group(),
            scope.recovery(),
            MIB,
        )?)
        .ok_or("old control absent")?,
        scope.group(),
        scope.recovery(),
        MIB,
    )?;
    let control = plan_control(old, header.identity, EntryStamp::Stamped(header.stamp))?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: header.request.clone(),
            intent: header.intent,
            original_position: at,
            original_time: control.execution_time(),
            expires_at: TimestampMillis::new(control.execution_time().get() + 1_000),
            status,
        },
        if profile == 1 { 2 } else { 1 },
        body,
        AdmissionLimits::default(),
    )?;
    let mut expected = expected;
    expected.insert(
        key::encode_outcome_key(&scope, header.request, MIB)?,
        value::encode_outcome(&outcome, 16 * MIB)?,
    );
    Ok(expected)
}

pub(super) fn record(
    id: &[u8],
    sequence: u128,
    due: Option<u64>,
) -> TestResult<StoredWorkflowRecord> {
    Ok(StoredWorkflowRecord {
        tenant: scope()?.tenant().clone(),
        partition: PartitionId::new(b"p".to_vec())?,
        record: restore_record(
            WorkflowRecordData {
                id: RecordId::new(id.to_vec())?,
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                version: 1,
                fence: Fence::new(0),
                attempts: 0,
                created_at: TimestampMillis::new(1),
                updated_at: TimestampMillis::new(1),
                next_run_at: due.map(TimestampMillis::new),
                run_state: None,
                lease: None,
                payload: Some(vec![0, 255]),
                result: None,
                error: None,
                latest_kind: EventKind::Created,
                latest_version: 1,
                latest_fence: Fence::new(0),
            },
            AdmissionLimits::default(),
        )?,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: sequence,
        },
    })
}
