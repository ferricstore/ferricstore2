//! Public preparation, both-mode identity and the closed captured-view parent.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, CompletePolicyReplaceApplyError as ExistingError,
    CompletePolicyRequestError as RequestError,
    OriginalCompletePolicyReplaceApplyError as RawError,
    OriginalCompletePolicyReplaceApplyView as RawView, OriginalEtfSafety,
    OriginalPolicyReadError as ReadError, OriginalPolicyReadValue as ReadValue,
    OriginalPolicySetApplyView, OriginalPolicySetFrameInput, OriginalPolicySetMapOrders,
    OriginalPolicySetRequest, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    RawFlowPolicyValue as Raw, prepare_original_policy_set_request,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{
    DispatchDecision, RequestScope, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    OriginalPolicySetRequestPlan as Plan, decide_original_policy_set_frame,
    prepare_original_policy_set_frame, verify_original_policy_set_frame,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::{TestResult, profile};

mod frame;
mod gate;
mod owners;
mod refusals;
mod response;
mod source;

const KEY: &[u8] = b"f:{f}:policy:r";
const EMPTY_PATCH: &[u8] = &[0, 0, 0, 17, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17];

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn stamp(time: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(time, 0)?, time)?)
}

const fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn empty_orders() -> OriginalPolicySetMapOrders<'static> {
    OriginalPolicySetMapOrders::new(
        digest(EMPTY_PATCH),
        None,
        &[(0, &[], &[])],
        &[],
        &[(0, &[], &[])],
    )
}

fn prepared(opts: Raw<'_>) -> TestResult<OriginalPolicySetRequest<'_>> {
    prepared_type(b"r", opts)
}

fn prepared_type<'input>(
    workflow_type: &'input [u8],
    opts: Raw<'input>,
) -> TestResult<OriginalPolicySetRequest<'input>> {
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    prepare_original_policy_set_request(&Raw::Binary(workflow_type), &opts, limits, &mut charge)?
        .map_err(|_| "valid public fixture refused".into())
}

fn frame_with(
    opts: Raw<'_>,
    orders: &OriginalPolicySetMapOrders<'_>,
    request: &[u8],
    time: u64,
) -> TestResult<Vec<u8>> {
    frame_for_type(b"r", opts, orders, request, time)
}

fn frame_for_type(
    workflow_type: &[u8],
    opts: Raw<'_>,
    orders: &OriginalPolicySetMapOrders<'_>,
    request: &[u8],
    time: u64,
) -> TestResult<Vec<u8>> {
    let limits = profile(2_000_000_000)?;
    let prepared = prepared_type(workflow_type, opts)?;
    let input = OriginalPolicySetFrameInput::new(
        scope()?,
        RequestId::new(request.to_vec())?,
        stamp(time)?,
        &prepared,
        orders,
    );
    let mut charge = PolicySchemaCharge::new(limits);
    Ok(
        prepare_original_policy_set_frame(input, limits, AdmissionLimits::default(), &mut charge)?
            .as_bytes()
            .to_vec(),
    )
}

fn frame(opts: Raw<'_>, orders: &OriginalPolicySetMapOrders<'_>) -> TestResult<Vec<u8>> {
    frame_with(opts, orders, b"q", 0)
}

fn view<'v, 'r, 'c>(
    gate: GateView<'v>,
    policies: &'r BTreeMap<Vec<u8>, ReadValue<'r>>,
    active: &'r BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    done: &'r BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    safety: OriginalEtfSafety<'c>,
) -> RawView<'v, 'r, 'c> {
    RawView {
        gate,
        revision: 7,
        policies,
        active_migrations: active,
        completed_migrations: done,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Ok(100),
        safety,
    }
}

fn apply<'view>(
    supplied: &RawView<'view, '_, '_>,
    encoded: &[u8],
    orders: &OriginalPolicySetMapOrders<'_>,
    index: u64,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Plan<'view>, RawError> {
    decide_original_policy_set_frame(
        OriginalPolicySetApplyView::new(*supplied, orders),
        encoded,
        log(index),
        limits,
        admission,
        charge,
    )
}

const fn safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(
        &[
            "flow_policy_v1",
            "type",
            "states",
            "max_active_ms",
            "a",
            "b",
        ],
        &[],
        None,
        &[],
    )
}

fn hex(encoded: &str) -> TestResult<Vec<u8>> {
    if !encoded.len().is_multiple_of(2) {
        return Err("odd fixture hex".into());
    }
    encoded
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn minimal(generation: u8) -> TestResult<Vec<u8>> {
    let mut bytes = hex(
        "836803770e666c6f775f706f6c6963795f763161017400000002770673746174657374000000007704747970656d0000000172",
    )?;
    bytes[20] = generation;
    Ok(bytes)
}

fn retained<'plan>(plan: &'plan Plan<'_>) -> TestResult<&'plan RetainedOutcome> {
    let DispatchDecision::Fresh((_, outcome)) = plan.decision() else {
        return Err("expected Fresh retained outcome".into());
    };
    Ok(outcome)
}

fn accepted(plan: &Plan<'_>, stored: u64, generation: u64, expected: &[u8]) -> TestResult {
    let DispatchDecision::Fresh((Some(install), outcome)) = plan.decision() else {
        return Err("expected independent installation owners".into());
    };
    assert_eq!(install.policy_key(), KEY);
    assert_eq!(install.stored_generation(), stored);
    assert_eq!(install.generation(), generation);
    assert_eq!(install.encoded(), expected);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), expected);
    assert_ne!(install.encoded().as_ptr(), outcome.response().as_ptr());
    assert_eq!(outcome.intent().version(), 4);
    Ok(())
}

#[test]
fn public_set_frame_preserves_the_full_public_preparation_profile() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let children = vec![Raw::List(&[]); 131_067];
    let pair = [Raw::Atom("x"), Raw::List(&children)];
    let opts = [Raw::Tuple(&pair)];
    // Public N131072, patch N131070 and literal node grammar, all before SUT.
    let patch_length = 2_228_187;
    let frame_length = 2_228_361;
    let mut expected_patch = Vec::with_capacity(patch_length);
    expected_patch.extend_from_slice(&u32::try_from(patch_length)?.to_be_bytes());
    expected_patch.extend_from_slice(&[4, 0, 0, 0, 0, 0, 0, 0, 1]);
    for offset in [25_u32, 35, 2_228_187] {
        expected_patch.extend_from_slice(&offset.to_be_bytes());
    }
    expected_patch.extend_from_slice(&[0, 0, 0, 10, 2, 0, 0, 0, 1, b'x']);
    append_empty_lists(&mut expected_patch, children.len())?;
    assert_eq!(expected_patch.len(), patch_length);
    let orders = OriginalPolicySetMapOrders::new(
        digest(&expected_patch),
        None,
        &[(0, &[0], &[0])],
        &[],
        &[],
    );
    let bytes = frame(Raw::List(&opts), &orders)?;
    assert_eq!(bytes.len(), frame_length);
    let mut charge = PolicySchemaCharge::new(limits);
    let verified = verify_original_policy_set_frame(
        &bytes,
        &orders,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(&verified.original_intent()[59..], expected_patch);
    assert_eq!(verified.header(log(0)).intent.version(), 4);
    frame::assert_reply_profile()?;
    Ok(())
}

fn append_empty_lists(output: &mut Vec<u8>, length: usize) -> TestResult {
    let bytes = 13 + 17 * length;
    output.extend_from_slice(&u32::try_from(bytes)?.to_be_bytes());
    output.push(5);
    output.extend_from_slice(&u32::try_from(length)?.to_be_bytes());
    let first = 13 + 4 * length;
    for index in 0..=length {
        output.extend_from_slice(&u32::try_from(first + 13 * index)?.to_be_bytes());
    }
    for _ in 0..length {
        output.extend_from_slice(&[0, 0, 0, 13, 5, 0, 0, 0, 0, 0, 0, 0, 13]);
    }
    Ok(())
}

mod reply;
