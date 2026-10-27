//! Real explicit stored publication inputs, observations and custody helpers.

use std::{collections::BTreeMap, path::Path};

use redb::{Durability, ReadableDatabase, ReadableTable};
use sha2::{Digest, Sha256};
use wf_domain::workflow::active::ActiveValueRef;
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::request::RetainedOutcome;
use wf_domain::workflow::{Create, Operation, StartAndClaim};
use wf_ports::blob::render_original_value_ref;
use wf_ports::blob::{OriginalValuePartition, OriginalValueRefKind, WorkflowValueBindingKnowledge};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch};
use wf_ports::log::encode_workflow_stored_value_batch;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StoredWorkflowRecord};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::apply_workflow_stored_value_batch_transaction;
use wf_storage::transaction::{RecordBatchTransactionOutcome, WorkflowValueTransactionLimits};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

mod cases;
mod custody;

fn fresh(path: &Path) -> TestResult<RedbStateStore> {
    let scope = crate::fixtures::scope()?;
    let placements = crate::fixtures::placements()?;
    Ok(RedbStateStore::create_stored_value_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        f::defaults(),
        StateStoreLimits::default(),
    )?)
}

fn open(path: &Path) -> TestResult<RedbStateStore> {
    let scope = crate::fixtures::scope()?;
    Ok(RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?)
}

fn batch(
    mode: u8,
    request: &[u8],
    now: u64,
    mut items: Vec<PreparedBatchItem>,
) -> TestResult<PreparedWorkflowValueBatch> {
    let count = items.len();
    let operation = if mode == 1 {
        PreparedOperation::PublicMutation {
            item: Box::new(items.pop().ok_or("public item")?),
            evidence: PublicMutationEvidence {
                requested_now_ms: Some(now),
                auto_partition: false,
                retry_override_present: false,
            },
        }
    } else {
        PreparedOperation::Batch(items)
    };
    let inputs = (0..count)
        .map(|i| {
            let mut input = f::input(u32::try_from(i)?, 0);
            input.requested_at = (mode == 1).then_some(TimestampMillis::new(now));
            Ok(input)
        })
        .collect::<TestResult<Vec<_>>>()?;
    f::batch(
        mode,
        request,
        now,
        operation,
        inputs,
        vec![f::witness(b"r", None)?],
    )
}

fn create(
    mode: u8,
    id: &[u8],
    now: u64,
    payload: Option<&[u8]>,
) -> TestResult<PreparedWorkflowValueBatch> {
    let operation = if mode == 7 {
        Operation::StartAndClaim(StartAndClaim {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            worker: b"w".to_vec(),
            lease_ms: 1_000,
            payload: payload.map(<[u8]>::to_vec),
        })
    } else {
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: payload.map(<[u8]>::to_vec),
        })
    };
    batch(mode, id, now, vec![f::item(id, now, operation)?])
}

fn apply(
    store: &mut RedbStateStore,
    request: &PreparedWorkflowValueBatch,
    at: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    apply_with(store, request, at, f::limits())
}

fn apply_with(
    store: &mut RedbStateStore,
    request: &PreparedWorkflowValueBatch,
    at: u64,
    limits: WorkflowValueTransactionLimits,
) -> TestResult<RecordBatchTransactionOutcome> {
    let encoded = encode_workflow_stored_value_batch(request, f::limits().record_batch.codec)?;
    Ok(apply_workflow_stored_value_batch_transaction(
        store,
        &request.scope,
        encoded.as_bytes(),
        crate::fixtures::log(at),
        limits,
    )?)
}

fn retained(result: RecordBatchTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    match result {
        RecordBatchTransactionOutcome::Retained { outcome, .. } => Ok(outcome),
        _ => Err("expected original retained outcome".into()),
    }
}

fn current(store: &mut RedbStateStore, id: &[u8]) -> TestResult<StoredWorkflowRecord> {
    let scope = crate::fixtures::scope()?;
    let limits = f::limits().record_batch;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    tx.record(
        scope.tenant(),
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
    )?
    .ok_or_else(|| "current record missing".into())
}

fn reference(id: &[u8], kind: OriginalValueRefKind, version: u64) -> TestResult<Vec<u8>> {
    render_original_value_ref(
        id,
        kind,
        version,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("reference: {error:?}").into())
}

fn assert_value(
    store: &RedbStateStore,
    id: &[u8],
    kind: OriginalValueRefKind,
    version: u64,
    expected: &[u8],
) -> TestResult {
    let reference = reference(id, kind, version)?;
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(300, 0)?,
        wall: TimestampMillis::new(300),
        calls: 0,
        failure: None,
    };
    let read = store.read_workflow_value_batch(
        &mut clock,
        &crate::fixtures::scope()?,
        &[&reference, &reference],
        f::read_limits(),
    )?;
    f::assert_read(&read, &[expected], &[Some(0), Some(0)])?;
    assert_eq!(clock.calls, 1);
    Ok(())
}

fn binding_versions(
    store: &RedbStateStore,
    id: &[u8],
    event: Option<&str>,
) -> TestResult<[Option<u64>; 3]> {
    let observed = store.workflow_value_binding(
        &crate::fixtures::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
        event,
        f::read_limits(),
    )?;
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = observed else {
        return Err("binding missing".into());
    };
    [
        binding.refs.payload,
        binding.refs.result,
        binding.refs.error,
    ]
    .map(|value| match value {
        None => Ok(None),
        Some(ActiveValueRef::Versioned { version, .. }) => Ok(Some(version)),
        Some(ActiveValueRef::Opaque(_)) => Err("unexpected opaque binding".into()),
    })
    .into_iter()
    .collect::<TestResult<Vec<_>>>()?
    .try_into()
    .map_err(|_| "binding count".into())
}

fn metadata(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(crate::fixtures::META)?;
    table
        .iter()?
        .map(|row| {
            let (k, v) = row?;
            Ok((k.value().to_vec(), v.value().to_vec()))
        })
        .collect()
}

fn rewrite(path: &Path, key: &[u8], bytes: &[u8], meta: bool) -> TestResult {
    let database = redb::Builder::new().open(path)?;
    let mut write = database.begin_write()?;
    write.set_durability(Durability::Immediate)?;
    let mut table = write.open_table(if meta {
        crate::fixtures::META
    } else {
        crate::fixtures::ROWS
    })?;
    assert!(table.insert(key, bytes)?.is_some());
    drop(table);
    write.commit()?;
    Ok(())
}

fn reseal(bytes: &mut [u8]) -> TestResult {
    let end = bytes.len().checked_sub(32).ok_or("checksum")?;
    let digest = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
    Ok(())
}
