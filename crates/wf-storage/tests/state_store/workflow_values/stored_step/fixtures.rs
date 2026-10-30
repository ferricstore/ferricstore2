use super::*;
use wf_domain::value::{
    OriginalValueContext, OriginalValueLimits, OriginalValueNode, decode_original_flow_value,
};
use wf_domain::workflow::Create;
use wf_ports::blob::{OriginalValuePartition, OriginalValueRefKind, render_original_value_ref};
use wf_ports::log::encode_workflow_stored_value_batch;
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::transaction::apply_workflow_stored_value_batch_transaction;
use wf_types::version::{HlcTimestamp, TimestampMillis};

pub(super) fn map(value: &[u8]) -> TestResult<Vec<u8>> {
    let mut bytes = b"FSV2\x02\x83t\0\0\0\x01w\x04stepm".to_vec();
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(super) fn seed(store: &mut RedbStateStore) -> TestResult<StoredWorkflowRecord> {
    mutation(
        store,
        b"create",
        1000,
        0,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1000)),
            payload: Some(map(b"queued")?),
        }),
    )?;
    let claim = c::phase(b"claim", 1000, b"queued", b"contract-step-worker", 1)?;
    assert_eq!(
        c::retained(c::apply(store, &claim, 1)?)?.status(),
        RetainedStatus::Accepted
    );
    c::record(store, b"i")
}

pub(super) fn mutation(
    store: &mut RedbStateStore,
    request: &[u8],
    now: u64,
    at: u64,
    operation: Operation,
) -> TestResult<Box<RetainedOutcome>> {
    let batch = v::batch(
        5,
        request,
        now,
        PreparedOperation::Batch(vec![v::item(b"i", now, operation)?]),
        vec![v::input(0, 0)],
        vec![v::witness(b"r", None)?],
    )?;
    let frame = encode_workflow_stored_value_batch(&batch, v::limits().record_batch.codec)?;
    c::retained_record(apply_workflow_stored_value_batch_transaction(
        store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(at),
        v::limits(),
    )?)
}

pub(super) fn frame(payload: Option<&[u8]>) -> TestResult<PreparedBytes> {
    let result = encode_stored_step_continue(
        request(payload)?,
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    );
    assert!(
        result.is_ok(),
        "healthy stored Step frame: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn request(payload: Option<&[u8]>) -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: f::scope()?,
        request: RequestId::new(b"step".to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: f::clock(1001)?,
        operation: PreparedOperation::Batch(vec![v::item(
            b"i",
            1001,
            Operation::StepContinue(StepContinue {
                guard: guard(b"contract-step-worker:1000:1", 1)?,
                from: b"queued".to_vec(),
                to: b"charge_card".to_vec(),
                worker: None,
                lease_ms: 5000,
                payload: payload.map(<[u8]>::to_vec),
            }),
        )?]),
    })
}

pub(super) fn guard(token: &[u8], fence: u64) -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: LeaseToken::new(token.to_vec())?,
        fence: Fence::new(fence),
    })
}

pub(super) fn apply(
    store: &mut RedbStateStore,
    frame: &PreparedBytes,
    at: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let result = apply_stored_step_continue_transaction(
        store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(at),
        v::limits(),
    );
    assert!(
        result.is_ok(),
        "healthy stored Step publication: {:?}",
        result.as_ref().err()
    );
    c::retained_record(result?)
}

pub(super) fn assert_map(store: &RedbStateStore, version: u64, expected: &[u8]) -> TestResult {
    let bytes = read_value(store, version)?;
    assert_eq!(bytes, map(expected)?);
    assert_document(bytes, expected)
}

pub(super) fn read_value(store: &RedbStateStore, version: u64) -> TestResult<Vec<u8>> {
    let reference = render_original_value_ref(
        b"i",
        OriginalValueRefKind::Payload,
        version,
        OriginalValuePartition::Explicit(b"p"),
        65535,
    )
    .map_err(|error| format!("reference: {error:?}"))?;
    let mut clock = v::ValueReadClock {
        hlc: HlcTimestamp::new(2000, 0)?,
        wall: TimestampMillis::new(2000),
        calls: 0,
        failure: None,
    };
    let mut result = store.read_workflow_value_batch(
        &mut clock,
        &f::scope()?,
        &[&reference],
        v::read_limits(),
    )?;
    assert_eq!(result.slots.len(), 1);
    assert_eq!(result.slots[0], Ok(Some(0)));
    assert_eq!(result.values.len(), 1);
    Ok(result.values.pop().ok_or("payload bytes")?)
}

fn assert_document(bytes: Vec<u8>, expected: &[u8]) -> TestResult {
    let document = decode_original_flow_value(
        bytes,
        &OriginalValueContext {
            safe_atoms: Some(&["step"]),
            active_exports: Some(&[]),
            local_node: None,
            local_functions: Some(&[]),
        },
        OriginalValueLimits {
            max_encoded_bytes: 4096,
            max_nodes: 32,
            max_edges: 32,
            max_depth: 8,
            max_work_units: 1_000_000,
        },
    )
    .map_err(|error| format!("typed payload: {error:?}"))?;
    let Some(OriginalValueNode::Map { entries }) = document.root() else {
        return Err("typed map payload".into());
    };
    let ids = document.edge_range(entries.clone()).ok_or("map entries")?;
    assert_eq!(ids.len(), 2);
    assert!(
        matches!(document.node(ids[0]), Some(OriginalValueNode::Atom { bytes, .. })
        if document.byte_range(bytes.clone()) == Some(b"step".as_slice()))
    );
    assert!(
        matches!(document.node(ids[1]), Some(OriginalValueNode::Binary { bytes })
        if document.byte_range(bytes.clone()) == Some(expected))
    );
    Ok(())
}

pub(super) fn assert_history(
    actual: &wf_ports::state::WorkflowHistoryPage,
    expected: &wf_ports::state::WorkflowHistoryPage,
) -> TestResult {
    assert_eq!(actual.control, expected.control);
    assert_eq!(actual.record_version, expected.record_version);
    assert_eq!(actual.continuation, expected.continuation);
    assert!(actual.lookahead.is_none() && expected.lookahead.is_none());
    assert_eq!(actual.entries.len(), expected.entries.len());
    assert_binding(&actual.current_values, &expected.current_values)?;
    for (left, right) in actual.entries.iter().zip(&expected.entries) {
        assert_eq!(left.event, right.event);
        assert_binding(&left.values, &right.values)?;
    }
    Ok(())
}

fn assert_binding(
    actual: &wf_ports::blob::WorkflowValueBindingKnowledge,
    expected: &wf_ports::blob::WorkflowValueBindingKnowledge,
) -> TestResult {
    use wf_ports::blob::WorkflowValueBindingKnowledge::Known;
    let (Known(Some(actual)), Known(Some(expected))) = (actual, expected) else {
        return Err("complete captured bindings expected".into());
    };
    assert!(actual.refs == expected.refs);
    assert!(actual.lifecycle == expected.lifecycle);
    Ok(())
}
