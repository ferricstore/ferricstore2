use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::caller::{
    LogicalMutationFieldValue as V, LogicalMutationInput, MutationFieldValue as S,
    PreparedLogicalMutation, prepare_logical_mutation,
};
use wf_domain::workflow::request::ClientIntent;
use wf_ports::consensus::WorkflowValueTargetGuard;
use wf_ports::log::{
    CodecLimits, LogicalMutationFrameContext as Context, LogicalMutationFrameLimits as Limits,
    PreparedBytes, encode_logical_mutation_frame,
};
use wf_types::identity::RequestId;
use wf_types::version::TimestampMillis;

use super::{TestResult, f};

pub(super) fn limits() -> TestResult<Limits> {
    Ok(Limits {
        codec: CodecLimits::new(AdmissionLimits::new(65_536, 1_048_576, 16)?),
        maximum_owned: 64 * 1_024 * 1_024,
        maximum_work: 512 * 1_024 * 1_024,
    })
}

pub(super) fn context(requested: Option<u64>, stamp: u64) -> TestResult<Context> {
    let mut input = f::input(0, 0);
    input.target = WorkflowValueTargetGuard::Absent;
    input.requested_at = requested.map(TimestampMillis::new);
    Ok(Context {
        scope: crate::fixtures::scope()?,
        request: RequestId::new(b"binding".to_vec())?,
        stamp: crate::fixtures::clock(stamp)?,
        policy_references_captured: Some(true),
        defaults: f::defaults(),
        policies: vec![f::witness(b"r", None)?],
        input,
    })
}

pub(super) fn caller(
    operation: &str,
    id: &[u8],
    partition: Option<&[u8]>,
    requested: Option<u64>,
) -> TestResult<PreparedLogicalMutation> {
    let mut fields = Vec::new();
    match operation {
        "create" => {
            fields.push(("type", V::Scalar(S::Bytes(b"r"))));
            fields.push(("state", V::Scalar(S::Bytes(b"q"))));
        }
        "transition" => {
            fields.push(("from", V::Scalar(S::Bytes(b"running"))));
            fields.push(("to", V::Scalar(S::Bytes(b"waiting"))));
        }
        _ => {}
    }
    if operation != "create" {
        fields.push(("fencing_token", V::Scalar(S::Integer(7))));
    }
    if !matches!(operation, "create" | "cancel") {
        fields.push(("lease_token", V::Scalar(S::Bytes(b"lease"))));
    }
    if let Some(partition) = partition {
        fields.push(("partition_key", V::Scalar(S::Bytes(partition))));
    }
    if let Some(now) = requested {
        fields.push(("now_ms", V::Scalar(S::Integer(i128::from(now)))));
    }
    if !matches!(operation, "extend_lease" | "cancel") {
        fields.push(("payload", V::Scalar(S::Bytes(b"FSV2\x02binary"))));
    }
    let secondary = match operation {
        "complete" => Some("result"),
        "retry" | "fail" => Some("error"),
        "cancel" => Some("reason"),
        _ => None,
    };
    if let Some(name) = secondary {
        fields.push((name, V::Scalar(S::Nil)));
    }
    Ok(prepare_logical_mutation(
        LogicalMutationInput {
            operation,
            id,
            fields: &fields,
        },
        &[],
        None,
        crate::fixtures::clock(100)?,
        super::super::logical_codec::limits(),
    )?)
}

pub(super) fn healthy(
    caller: PreparedLogicalMutation,
    context: Context,
    limits: Limits,
) -> TestResult<(PreparedBytes, ClientIntent)> {
    let result = encode_logical_mutation_frame(caller, context, limits);
    assert!(
        result.is_ok(),
        "healthy logical frame binding: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

pub(in super::super) fn automatic(id: &[u8]) -> TestResult<(PreparedBytes, ClientIntent)> {
    healthy(
        caller("create", id, None, None)?,
        context(None, 100)?,
        limits()?,
    )
}
