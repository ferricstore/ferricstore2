//! Real logged Complete and Original SET donors, never direct table installation.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, OriginalPolicyReadValue,
    OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, PolicySchemaCharge,
    RawFlowPolicyValue as Raw, StoredPolicyValue, canonical_original_etf_safety,
    decode_original_complete_policy_entry_portable, prepare_original_policy_set_frame_bytes,
    prepare_original_policy_set_request,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, StateChange, StatePolicyPatch};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::consensus::WorkflowValuePolicyWitness;
use wf_ports::log::{
    PreparedBytes, RaftLogStore, prepare_complete_core_policy_patch_frame,
    prepare_original_complete_policy_reference, prepare_original_policy_set_frame,
};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::log::RedbLogStore;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_complete_core_policy_patch_transaction,
    apply_original_policy_set_transaction,
};
use wf_types::identity::RequestId;

use super::super::{TestResult, f};
use super::limits;

pub(in crate::workflow_values) fn policy_frame(
    original: bool,
    expected: Option<u64>,
    at: u64,
    now: u64,
) -> TestResult<PreparedBytes> {
    if original {
        return original_frame(expected, at, now);
    }
    let schema = limits()?.normal_limits().schema;
    let patch = PolicyPatch {
        max_active_ms: FieldPatch::Set(5_000),
        retention: FieldPatch::Set(RetentionOverride {
            ttl_ms: Some(11_000),
            history_max_events: Some(30),
        }),
        states: BTreeMap::from([(
            b"q".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                retention: FieldPatch::Set(RetentionOverride {
                    ttl_ms: Some(if expected.is_some() { 88_000 } else { 22_000 }),
                    history_max_events: Some(40),
                }),
                ..Default::default()
            }),
        )]),
        ..Default::default()
    };
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            f::scope()?,
            RequestId::new(format!("complete-policy-{at}").into_bytes())?,
            f::clock(now, 0, now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: expected,
                patch,
            },
        ),
        schema,
        limits()?.normal_limits().codec.admission(),
        &mut PolicySchemaCharge::new(schema),
    )?)
}

fn original_frame(expected: Option<u64>, at: u64, now: u64) -> TestResult<PreparedBytes> {
    let limits = limits()?.normal_limits();
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let type_retention = [
        (Raw::Atom("history_max_events"), Raw::Integer(30)),
        (Raw::Atom("ttl_ms"), Raw::Integer(11_000)),
    ];
    let state_retention = [
        (Raw::Atom("history_max_events"), Raw::Integer(40)),
        (Raw::Atom("ttl_ms"), Raw::Integer(22_000)),
    ];
    let config = [(Raw::Atom("retention"), Raw::Map(&state_retention))];
    let states = [(Raw::Binary(b"q"), Raw::Map(&config))];
    let root = [
        [Raw::Atom("max_active_ms"), Raw::Integer(5_000)],
        [Raw::Atom("retention"), Raw::Map(&type_retention)],
        [Raw::Atom("states"), Raw::Map(&states)],
    ];
    let controls = [
        [Raw::Atom("replace"), Raw::Atom("true")],
        [
            Raw::Atom("expected_generation"),
            Raw::Integer(expected.map_or(0, i128::from)),
        ],
    ];
    let mut options = root.iter().map(|pair| Raw::Tuple(pair)).collect::<Vec<_>>();
    options.push(Raw::Tuple(&controls[0]));
    if expected.is_some() {
        options.push(Raw::Tuple(&controls[1]));
    }
    let prepared = prepare_original_policy_set_request(
        &Raw::Binary(b"r"),
        &Raw::List(&options),
        limits.schema,
        &mut charge,
    )?
    .map_err(|error| format!("fixed Original SET caller rejected: {error:?}"))?;
    let maps = [
        (0, [0, 1, 2].as_slice(), [0, 1, 2].as_slice()),
        (1, [0, 1].as_slice(), [0, 1].as_slice()),
        (2, [0].as_slice(), [0].as_slice()),
        (3, [0].as_slice(), [0].as_slice()),
        (4, [0, 1].as_slice(), [0, 1].as_slice()),
    ];
    let scope = f::scope()?;
    let request = RequestId::new(format!("original-policy-{at}").into_bytes())?;
    let stamp = f::clock(now, 0, now)?;
    let initial = OriginalPolicySetMapOrders::new([0; 32], None, &maps, &[], &[]);
    let (bytes, _, _, range) = prepare_original_policy_set_frame_bytes(
        OriginalPolicySetFrameInput::new(
            scope.clone(),
            request.clone(),
            stamp,
            &prepared,
            &initial,
        ),
        limits.schema,
        limits.codec.admission(),
        &mut charge,
    )?;
    let orders = OriginalPolicySetMapOrders::new(
        Sha256::digest(&bytes[range]).into(),
        None,
        &maps,
        &[],
        &[],
    );
    Ok(prepare_original_policy_set_frame(
        OriginalPolicySetFrameInput::new(scope, request, stamp, &prepared, &orders),
        limits.schema,
        limits.codec.admission(),
        &mut charge,
    )?)
}

pub(in crate::workflow_values) fn install(
    log: &mut RedbLogStore,
    state: &mut RedbStateStore,
    original: bool,
    expected: Option<u64>,
    at: u64,
    now: u64,
) -> TestResult<WorkflowValuePolicyWitness> {
    let frame = policy_frame(original, expected, at, now)?;
    f::append(log, at, frame.as_bytes())?;
    let scope = f::scope()?;
    let limits = limits()?.normal_limits();
    let result = if original {
        apply_original_policy_set_transaction(
            state,
            &scope,
            frame.as_bytes(),
            f::at(at),
            limits,
            &mut PolicySchemaCharge::new(limits.schema),
        )?
    } else {
        apply_complete_core_policy_patch_transaction(
            state,
            &scope,
            frame.as_bytes(),
            f::at(at),
            limits,
        )?
    };
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("real policy installation did not retain acceptance".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), f::at(at));
    log.save_committed(f::at(at))?;
    let value = load_policy(state)?.ok_or("installed full policy missing")?;
    let witness = witness(Some(&value))?;
    assert_eq!(witness.profile, if original { 2 } else { 1 });
    assert_eq!(witness.generation, expected.unwrap_or(0) + 1);
    Ok(witness)
}

pub(in crate::workflow_values) fn load_policy(
    state: &mut RedbStateStore,
) -> TestResult<Option<StoredPolicyValue>> {
    let scope = f::scope()?;
    let limits = limits()?.normal_limits();
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let value = tx.stored_policy_value(
        scope.tenant(),
        b"r",
        limits.schema,
        &mut PolicySchemaCharge::new(limits.schema),
    )?;
    drop(tx);
    Ok(value)
}

pub(in crate::workflow_values) fn witness(
    policy: Option<&StoredPolicyValue>,
) -> TestResult<WorkflowValuePolicyWitness> {
    let schema = limits()?.normal_limits().schema;
    let mut charge = PolicySchemaCharge::new(schema);
    let (profile, generation, digest) = match policy {
        None => {
            let reference = prepare_original_complete_policy_reference(
                OriginalPolicyReadValue::Absent,
                b"r",
                &canonical_original_etf_safety(),
                schema,
                &mut charge,
            )?;
            assert_eq!(
                reference.digest().as_bytes().as_slice(),
                f::hex("6e9978636a6017b6e2abf81647a5aab1b71f003b16e23ad931a1beda2d62b9a9")?,
            );
            (0, 0, reference.digest())
        }
        Some(StoredPolicyValue::Complete(value)) => (
            1,
            value.generation(),
            PreparedDigest::new(Sha256::digest(value.encoded()).into()),
        ),
        Some(StoredPolicyValue::Original(bytes)) => {
            let value = decode_original_complete_policy_entry_portable(bytes, schema, &mut charge)?
                .ok_or("Original full donor is unsupported")?;
            assert_eq!(value.workflow_type(), Some(b"r".as_slice()));
            (
                2,
                value.generation(),
                PreparedDigest::new(Sha256::digest(bytes).into()),
            )
        }
    };
    Ok(WorkflowValuePolicyWitness {
        workflow_type: b"r".to_vec(),
        profile,
        generation,
        digest,
    })
}
