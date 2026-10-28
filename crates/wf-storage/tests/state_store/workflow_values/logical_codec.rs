//! Logical caller composition with the actual intent9 codec, no ledger credit.

use wf_domain::value::{
    OriginalValueEncodeLimits, OriginalValueInput as I, OriginalValueNode as N,
};
use wf_domain::workflow::Operation;
use wf_domain::workflow::caller::{
    LogicalMutationFieldValue as V, LogicalMutationInput, LogicalMutationLimits,
    MutationFieldValue as S, PreparedLogicalMutation, prepare_logical_mutation,
};
use wf_ports::consensus::{
    PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueTargetGuard,
};
use wf_ports::log::{
    decode_prepared, decode_workflow_stored_value_batch, encode_workflow_stored_value_batch,
};
use wf_types::identity::PartitionId;

use super::{TestResult, fixtures as f};

pub(super) fn limits() -> LogicalMutationLimits {
    LogicalMutationLimits {
        admission: f::limits().record_batch.codec.admission(),
        values: OriginalValueEncodeLimits {
            max_input_bytes: 65_536,
            max_nodes: 4_096,
            max_edges: 8_192,
            max_depth: 128,
            max_encoded_bytes: 65_536,
            max_work_units: 4_194_304,
            max_temporary_bytes: 4_194_304,
        },
        max_raw_bytes: 4_194_304,
        max_work_units: 16_777_216,
        max_temporary_bytes: 16_777_216,
    }
}

pub(super) fn prepared_create(id: &[u8], payload: V<'_>) -> TestResult<PreparedLogicalMutation> {
    let fields = [
        ("type", V::Scalar(S::Bytes(b"r"))),
        ("state", V::Scalar(S::Bytes(b"q"))),
        ("partition_key", V::Scalar(S::Bytes(b"p"))),
        ("payload", payload),
    ];
    let result = prepare_logical_mutation(
        LogicalMutationInput {
            operation: "create",
            id,
            fields: &fields,
        },
        &[],
        None,
        crate::fixtures::clock(100)?,
        limits(),
    );
    assert!(
        result.is_ok(),
        "healthy stored logical caller: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(super) fn request(
    value: PreparedLogicalMutation,
    name: &[u8],
) -> TestResult<PreparedWorkflowValueBatch> {
    let (partition, command, create_priority, evidence) = value.into_stored_parts();
    let item = PreparedBatchItem {
        partition: PartitionId::new(partition.ok_or("explicit partition")?)?,
        command,
        create_priority,
    };
    let mut input = f::input(0, 0);
    input.target = WorkflowValueTargetGuard::Absent;
    f::batch(
        1,
        name,
        100,
        PreparedOperation::PublicMutation {
            item: Box::new(item),
            evidence,
        },
        vec![input],
        vec![f::witness(b"r", None)?],
    )
}

#[test]
fn logical_caller_enters_stored_codec_without_binary_reinterpretation() -> TestResult {
    let nodes = [
        N::Integer(7),
        N::Binary { bytes: 0..1 },
        N::Tuple { children: 0..2 },
    ];
    let value = V::Value {
        input: I {
            bytes: b"x",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        init_pid: None,
    };
    let typed = prepared_create(b"i", value)?;
    let expected = b"FSV2\x02\x83h\x02a\x07m\0\0\0\x01x";
    assert!(
        matches!(&typed.command().operation, Operation::Create(create) if create.payload.as_deref() == Some(expected.as_slice()))
    );
    let limits = f::limits().record_batch.codec;
    let typed = request(typed, b"logical-codec")?;
    let first = encode_workflow_stored_value_batch(&typed, limits)?;
    let generic = decode_prepared(first.as_bytes(), limits)?;
    assert_eq!(generic.request().intent.version(), 9);
    assert_eq!(
        generic.value_bytes(0),
        Some([Some(expected.as_slice()), None])
    );
    let decoded = decode_workflow_stored_value_batch(first.as_bytes(), limits)?;
    assert!(decoded.operation == typed.operation);
    let binary = request(
        prepared_create(b"i", V::Scalar(S::Bytes(expected)))?,
        b"logical-codec",
    )?;
    let second = encode_workflow_stored_value_batch(&binary, limits)?;
    let other = decode_prepared(second.as_bytes(), limits)?;
    assert_ne!(generic.request().intent, other.request().intent);
    let mut escaped = b"FSV2\x01".to_vec();
    escaped.extend_from_slice(expected);
    assert_eq!(other.value_bytes(0), Some([Some(escaped.as_slice()), None]));
    Ok(())
}
