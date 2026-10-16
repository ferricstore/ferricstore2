//! Public V1 item framing and the one mathematical retained gate.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{ClaimRecord, ScopedRecordId};
use wf_domain::workflow::DomainError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::caller::{
    MutationFieldValue, MutationInput, NormalizedMutation, normalize_core_mutation,
};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::{
    IndependentIndexedDecision, IndexRejection, TouchedIndexedGroupView,
};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    CodecLimits, PreparedBytes, RecordBatchApplyDecision, RecordBatchApplyError,
    RecordBatchApplyPlan, RecordBatchInspection, decide_fresh_public_mutation_touched,
    encode_public_mutation, inspect_public_mutation_point, measure_public_mutation,
    prepare_public_mutation, prepare_public_mutation_intent, public_mutation_response_matches,
    public_mutation_retained_status,
};
use wf_types::identity::{GroupId, PartitionId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod gate;
mod wire;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const WORKSPACE: usize = 64 * 1_024 * 1_024;
const FRAME: &str = "4657435000010400000001740000000167000000017a00000001780001ad27fb04f2617b292c86a42ad3e31ef6346e62938f4586d81588f1c76ad5846600000000000007d0000000000000000007d0000100000000017000000001690000000000000007d005000000016c00000000000000010000";

fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn hex(text: &str) -> TestResult<Vec<u8>> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn fields() -> [(&'static str, MutationFieldValue<'static>); 2] {
    [
        ("lease_token", MutationFieldValue::Bytes(b"l")),
        ("fencing_token", MutationFieldValue::Integer(1)),
    ]
}

fn caller<'a>(
    fields: &'a [(&'a str, MutationFieldValue<'a>)],
    partition: Option<&'a [u8]>,
) -> TestResult<NormalizedMutation<'a>> {
    Ok(normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: b"i",
            fields,
        },
        &[],
        partition,
        AdmissionLimits::default(),
        16 * 1_024 * 1_024,
    )?)
}

fn encoded(
    now: u64,
    fields: &[(&str, MutationFieldValue<'_>)],
) -> TestResult<(PreparedBytes, ClientIntent)> {
    Ok(encode_public_mutation(
        &scope()?,
        &RequestId::new(b"x".to_vec())?,
        &caller(fields, Some(b"p"))?,
        clock(now)?,
        limits(),
        WORKSPACE,
    )?)
}

fn inspect<'input, 'retained>(
    bytes: &'input [u8],
    control: ControlState,
    original: Option<&'retained RetainedOutcome>,
    index: u64,
) -> TestResult<RecordBatchInspection<'input, 'retained>> {
    let scope = scope()?;
    Ok(inspect_public_mutation_point(
        prepare_public_mutation(bytes, limits(), WORKSPACE)?,
        log(index),
        PointGateView {
            scope: &scope,
            control,
            outcome: original,
            retained_nonempty: original.is_some(),
        },
        limits(),
        WORKSPACE,
    )?)
}

fn decide_missing(bytes: &[u8], workspace: usize) -> TestResult<RecordBatchApplyPlan<'static>> {
    decide_missing_at(bytes, workspace, ControlState::initial(clock(0)?))
}

fn decide_missing_at(
    bytes: &[u8],
    workspace: usize,
    control: ControlState,
) -> TestResult<RecordBatchApplyPlan<'static>> {
    let scope = scope()?;
    let decoded = prepare_public_mutation(bytes, limits(), workspace)?;
    let inspection = inspect_public_mutation_point(
        decoded,
        log(0),
        PointGateView {
            scope: &scope,
            control,
            outcome: None,
            retained_nonempty: false,
        },
        limits(),
        workspace,
    )?;
    let RecordBatchInspection::Fresh(fresh) = inspection else {
        return Err("not Fresh".into());
    };
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, scope.group().clone())]);
    let current = BTreeMap::from([(
        ScopedRecordId {
            partition_key: b"p",
            id: b"i",
        },
        None::<ClaimRecord<'static>>,
    )]);
    let empty_parallel = BTreeMap::new();
    let empty_members = BTreeMap::new();
    let empty_heads = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = BTreeMap::from([(b"r".as_slice(), None)]);
    Ok(decide_fresh_public_mutation_touched(
        fresh,
        TouchedIndexedGroupView {
            group: scope.group(),
            placements: &placements,
            current: &current,
            parallel: &empty_parallel,
            fifo_members: &empty_members,
            fifo_heads: &empty_heads,
            policies: &policies,
            retry_policies: &retries,
        },
        0,
        1_000,
    )?)
}

fn retained<'plan>(plan: &'plan RecordBatchApplyPlan<'_>) -> TestResult<&'plan RetainedOutcome> {
    let RecordBatchApplyDecision::IndependentFresh { outcome, .. } = plan.decision() else {
        return Err("not public independent Fresh".into());
    };
    Ok(outcome)
}

fn altered(
    original: &RetainedOutcome,
    status: RetainedStatus,
    body: Vec<u8>,
) -> TestResult<RetainedOutcome> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: original.scope().clone(),
            request: original.request().clone(),
            intent: original.intent(),
            original_position: original.original_position(),
            original_time: original.original_time(),
            expires_at: original.expires_at(),
            status,
        },
        2,
        body,
        AdmissionLimits::default(),
    )?)
}
