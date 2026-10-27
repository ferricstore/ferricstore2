//! Actual stored history producers and independent source-field assertions.

use std::{collections::BTreeMap, path::Path};

use redb::{ReadableDatabase, ReadableTable};
use wf_domain::history::values::{HistoryValue, HistoryValueLimits, HistoryValueReturn};
use wf_domain::value::{
    OriginalValueContext, OriginalValueDocument, OriginalValueLimits, OriginalValueNode,
};
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::{Cancel, Create, Operation, Transition};
use wf_ports::blob::{HistoryValueReadConversionLimits, WorkflowValueReadFailure};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch};
use wf_ports::log::encode_workflow_stored_value_batch;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryDirection, WorkflowHistoryHydrationError as Error,
    WorkflowHistoryHydrationLimits, WorkflowHistoryHydrationRequest, WorkflowHistoryQuery,
    WorkflowHistoryReadLimits, WorkflowHistoryValuePage,
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_workflow_stored_value_batch_transaction,
};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{Fence, HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

mod boundaries;
mod custody;
mod source;
mod values;

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

fn limits() -> WorkflowHistoryHydrationLimits {
    WorkflowHistoryHydrationLimits {
        history: WorkflowHistoryReadLimits {
            maximum_entries: 1_000,
            maximum_row_reads: 10_000,
            maximum_inspected_bytes: 64 * crate::fixtures::MIB,
            maximum_returned_bytes: 64 * crate::fixtures::MIB,
            maximum_work: 64 * crate::fixtures::MIB,
            maximum_temporary_bytes: 64 * crate::fixtures::MIB,
        },
        values: f::read_limits(),
        conversion: HistoryValueReadConversionLimits {
            maximum_references: 192,
            maximum_work: 1_000_000,
            maximum_temporary_bytes: 8_000_000,
        },
        projection: HistoryValueLimits {
            maximum_entries: 64,
            maximum_references: 192,
            maximum_read_results: 192,
            maximum_writes: 384,
            maximum_work_units: 20_000_000,
            maximum_requested_bytes: 128_000_000,
            decoder: OriginalValueLimits {
                max_encoded_bytes: 262_144,
                max_nodes: 4_096,
                max_edges: 8_192,
                max_depth: 256,
                max_work_units: 200_000,
            },
        },
        maximum_work: 512 * crate::fixtures::MIB,
        maximum_temporary_bytes: 512 * crate::fixtures::MIB,
    }
}

fn context() -> OriginalValueContext<'static> {
    OriginalValueContext {
        safe_atoms: None,
        active_exports: None,
        local_node: None,
        local_functions: None,
    }
}

fn selection(
    limit: usize,
    value_return: HistoryValueReturn,
) -> WorkflowHistoryHydrationRequest<'static> {
    WorkflowHistoryHydrationRequest {
        query: WorkflowHistoryQuery {
            limit,
            boundary_event: None,
            direction: WorkflowHistoryDirection::Ascending,
            expected_control: None,
        },
        value_return,
        decode_context: context(),
    }
}

fn clock(now: u64) -> TestResult<f::ValueReadClock> {
    Ok(f::ValueReadClock {
        hlc: HlcTimestamp::new(now, 0)?,
        wall: TimestampMillis::new(now),
        calls: 0,
        failure: None,
    })
}

fn read(
    store: &RedbStateStore,
    clock: &mut f::ValueReadClock,
    id: &[u8],
    request: WorkflowHistoryHydrationRequest<'_>,
) -> TestResult<WorkflowHistoryValuePage> {
    let result = store.read_workflow_history_page_with_values(
        clock,
        &crate::fixtures::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
        request,
        limits(),
    );
    assert!(
        result.is_ok(),
        "healthy history hydration: {:?}",
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

fn cancel(id: &[u8], state: &[u8], version: u64, error: &[u8]) -> TestResult<PreparedBatchItem> {
    f::item(
        id,
        100,
        Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: Some(state.to_vec()),
            expected_version: Some(version),
            error: Some(error.to_vec()),
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
        return Err("retained publication missing".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(())
}

fn field<'a>(
    page: &'a WorkflowHistoryValuePage,
    entry: usize,
    key: &str,
) -> TestResult<&'a HistoryValue> {
    page.writes
        .iter()
        .find(|write| write.entry_index == entry && write.field.key() == key)
        .map(|write| &write.value)
        .ok_or_else(|| format!("missing field {entry}:{key}").into())
}

fn document<'a>(
    page: &'a WorkflowHistoryValuePage,
    entry: usize,
    key: &str,
) -> TestResult<&'a OriginalValueDocument> {
    match field(page, entry, key)? {
        HistoryValue::Decoded(document) => Ok(document),
        _ => Err("expected decoded document".into()),
    }
}

fn binary(page: &WorkflowHistoryValuePage, entry: usize, key: &str, expected: &[u8]) -> TestResult {
    let document = document(page, entry, key)?;
    assert!(
        matches!(document.root(), Some(OriginalValueNode::Binary { bytes }) if document.byte_range(bytes.clone()) == Some(expected))
    );
    Ok(())
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
