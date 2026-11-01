use super::*;
use std::path::Path;
use wf_domain::workflow::{Create, Operation};
use wf_ports::consensus::{
    PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueTargetGuard,
};
use wf_ports::log::{
    MetadataBatchInput, MetadataBatchLimits, PreparedBytes, encode_workflow_metadata_batch,
    encode_workflow_stored_value_batch,
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_workflow_metadata_batch_transaction,
};

pub(super) fn metadata(value: u8) -> Vec<u8> {
    let mut bytes = b"FSV2\x02\x83\x68\x03\x6d\0\0\0\x12wf.record.metadata\x61\x01\x74\0\0\0\x01\x6d\0\0\0\x0aattributes\x61".to_vec();
    bytes.push(value);
    bytes
}
pub(super) fn fresh(path: &Path) -> TestResult<RedbStateStore> {
    let scope = crate::fixtures::scope()?;
    let placements = crate::fixtures::placements()?;
    Ok(RedbStateStore::create_metadata_value_database(
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

pub(super) fn legacy_fresh(path: &Path) -> TestResult<RedbStateStore> {
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

pub(super) fn transition(
    store: &RedbStateStore,
    id: &[u8],
    key: &[u8],
    now: u64,
    from: &[u8],
    to: &[u8],
) -> TestResult<PreparedWorkflowValueBatch> {
    use wf_ports::blob::WorkflowValueBindingKnowledge;
    use wf_types::{
        identity::{PartitionId, RecordId},
        version::Fence,
    };
    let scope = crate::fixtures::scope()?;
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = store.workflow_value_binding(
        &scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
        None,
        f::read_limits(),
    )?
    else {
        return Err("missing actual metadata binding".into());
    };
    let item = f::item(
        id,
        now,
        Operation::Transition(wf_domain::workflow::Transition {
            from: from.to_vec(),
            to: to.to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: None,
        }),
    )?;
    let mut input = f::input(0, 0);
    input.target = WorkflowValueTargetGuard::Present {
        incarnation: binding.lifecycle.incarnation,
    };
    f::batch(
        5,
        key,
        now,
        PreparedOperation::Batch(vec![item]),
        vec![input],
        vec![f::witness(b"r", None)?],
    )
}

pub(super) fn history_ids(path: &Path) -> TestResult<Vec<String>> {
    use wf_storage::state::value::{HistorySnapshotLimits, decode_history_snapshot};
    use wf_types::identity::{PartitionId, RecordId};
    let scope = crate::fixtures::scope()?;
    let rows = crate::fixtures::raw_rows(path)?;
    let cap = HistorySnapshotLimits::new(
        codec_limits().codec.admission(),
        crate::fixtures::MIB,
        4 * crate::fixtures::MIB,
        8 * crate::fixtures::MIB,
    )?;
    let mut events = rows
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&5))
        .map(|(_, bytes)| {
            let snapshot = decode_history_snapshot(
                bytes,
                &scope,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(b"i".to_vec())?,
                cap,
            )?;
            Ok((snapshot.postimage.record.version(), snapshot.event.event_id))
        })
        .collect::<TestResult<Vec<_>>>()?;
    events.sort_by_key(|(version, _)| *version);
    Ok(events.into_iter().map(|(_, event)| event).collect())
}

pub(super) fn request(id: &[u8], key: &[u8], now: u64) -> TestResult<PreparedWorkflowValueBatch> {
    let item = f::item(
        id,
        now,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(b"data".to_vec()),
        }),
    )?;
    let mut input = f::input(0, 0);
    input.target = WorkflowValueTargetGuard::Absent;
    f::batch(
        5,
        key,
        now,
        PreparedOperation::Batch(vec![item]),
        vec![input],
        vec![f::witness(b"r", None)?],
    )
}
pub(super) fn codec_limits() -> MetadataBatchLimits {
    MetadataBatchLimits {
        codec: f::limits().record_batch.codec,
        maximum_owned_bytes: 64 * crate::fixtures::MIB,
        maximum_work_units: 64 * crate::fixtures::MIB,
    }
}
pub(super) fn encode(
    batch: &PreparedWorkflowValueBatch,
    values: &[Option<&[u8]>],
) -> TestResult<PreparedBytes> {
    let inner = encode_workflow_stored_value_batch(batch, codec_limits().codec)?;
    Ok(encode_workflow_metadata_batch(
        MetadataBatchInput {
            stored_batch: inner.as_bytes(),
            metadata: values,
        },
        codec_limits(),
    )?)
}
pub(super) fn apply(
    store: &mut RedbStateStore,
    bytes: &PreparedBytes,
    at: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    Ok(apply_workflow_metadata_batch_transaction(
        store,
        &crate::fixtures::scope()?,
        bytes.as_bytes(),
        crate::fixtures::log(at),
        f::limits(),
    )?)
}

pub(super) fn replace_rows(
    path: &Path,
    rows: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult {
    let db = redb::Builder::new().open(path)?;
    let mut tx = db.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    {
        let mut table = tx.open_table(crate::fixtures::ROWS)?;
        table.retain(|_, _| false)?;
        for (key, bytes) in rows {
            table.insert(key.as_slice(), bytes.as_slice())?;
        }
    }
    tx.commit()?;
    Ok(())
}
pub(super) fn reseal(bytes: &mut Vec<u8>) {
    use sha2::{Digest, Sha256};
    bytes.truncate(bytes.len() - 32);
    let sum: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&sum);
}
