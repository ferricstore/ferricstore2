//! Real producer inputs and owned physical-page observations.

use std::{collections::BTreeMap, path::Path};

use redb::{ReadableDatabase, ReadableTable};
use wf_domain::workflow::active::ActiveValueRef;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_domain::workflow::{Create, Operation, Transition};
use wf_ports::blob::WorkflowValueBindingKnowledge;
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch};
use wf_ports::log::encode_workflow_stored_value_batch;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryDirection, WorkflowHistoryPage, WorkflowHistoryQuery,
    WorkflowHistoryReadLimits,
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_workflow_stored_value_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::Fence;

use super::{TestResult, fixtures as f};

mod closure;
mod custody;
mod order;

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

fn limits() -> WorkflowHistoryReadLimits {
    WorkflowHistoryReadLimits {
        maximum_entries: 1_000,
        maximum_row_reads: 10_000,
        maximum_inspected_bytes: 64 * crate::fixtures::MIB,
        maximum_returned_bytes: 64 * crate::fixtures::MIB,
        maximum_work: 64 * crate::fixtures::MIB,
        maximum_temporary_bytes: 64 * crate::fixtures::MIB,
    }
}

fn query(limit: usize) -> WorkflowHistoryQuery<'static> {
    WorkflowHistoryQuery {
        limit,
        boundary_event: None,
        direction: WorkflowHistoryDirection::Ascending,
        expected_control: None,
    }
}

fn read(
    store: &RedbStateStore,
    scope: &RequestScope,
    id: &[u8],
    query: WorkflowHistoryQuery<'_>,
) -> TestResult<WorkflowHistoryPage> {
    let result = store.read_workflow_history_page(
        scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
        query,
        limits(),
    );
    assert!(
        result.is_ok(),
        "healthy history capture: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

fn create(id: &[u8], payload: Option<&[u8]>) -> TestResult<PreparedBatchItem> {
    f::item(
        id,
        100,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: payload.map(<[u8]>::to_vec),
        }),
    )
}

fn transition(
    id: &[u8],
    from: &[u8],
    to: &[u8],
    payload: Option<&[u8]>,
) -> TestResult<PreparedBatchItem> {
    f::item(
        id,
        100,
        Operation::Transition(Transition {
            from: from.to_vec(),
            to: to.to_vec(),
            fence: Fence::new(0),
            lease_token: None,
            run_at: None,
            payload: payload.map(<[u8]>::to_vec),
        }),
    )
}

fn batch(request: &[u8], items: Vec<PreparedBatchItem>) -> TestResult<PreparedWorkflowValueBatch> {
    let inputs = (0..items.len())
        .map(|i| Ok(f::input(u32::try_from(i)?, 0)))
        .collect::<TestResult<Vec<_>>>()?;
    f::batch(
        6,
        request,
        100,
        PreparedOperation::Batch(items),
        inputs,
        vec![f::witness(b"r", None)?],
    )
}

fn apply(store: &mut RedbStateStore, request: &PreparedWorkflowValueBatch, at: u64) -> TestResult {
    let frame = encode_workflow_stored_value_batch(request, f::limits().record_batch.codec)?;
    let result = apply_workflow_stored_value_batch_transaction(
        store,
        &request.scope,
        frame.as_bytes(),
        crate::fixtures::log(at),
        f::limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("missing actual retained publication".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(())
}

fn ids(page: &WorkflowHistoryPage) -> Vec<&str> {
    page.entries
        .iter()
        .map(|row| row.event.event_id.as_str())
        .collect()
}

fn versions(knowledge: &WorkflowValueBindingKnowledge) -> TestResult<[Option<u64>; 3]> {
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = knowledge else {
        return Err("missing actual binding".into());
    };
    let mut result = [None; 3];
    for (target, field) in result.iter_mut().zip([
        &binding.refs.payload,
        &binding.refs.result,
        &binding.refs.error,
    ]) {
        if let Some(ActiveValueRef::Versioned { version, .. }) = field {
            *target = Some(*version);
        } else {
            assert!(field.is_none());
        }
    }
    Ok(result)
}

fn metadata(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let db = redb::Builder::new().open(path)?;
    let read = db.begin_read()?;
    let table = read.open_table(crate::fixtures::META)?;
    table
        .iter()?
        .map(|row| {
            let (key, value) = row?;
            Ok((key.value().to_vec(), value.value().to_vec()))
        })
        .collect()
}
