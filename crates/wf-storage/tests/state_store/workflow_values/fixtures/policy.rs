//! Accepted full policy inputs and genuine logged policy publication.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, CompletePolicyData,
    CompletePolicyValue, CompleteStatePolicyData, OriginalPolicyReadValue,
    OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, PolicySchemaCharge,
    RawFlowPolicyValue as Raw, StoredPolicyValue, canonical_original_etf_safety,
    decode_original_complete_policy_entry_portable, prepare_complete_policy_value_v2,
    prepare_original_policy_set_frame_bytes, prepare_original_policy_set_request,
    restore_complete_policy, verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::PolicyPatch;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::consensus::WorkflowValuePolicyWitness;
use wf_ports::log::{
    PreparedBytes, prepare_complete_core_policy_patch_frame,
    prepare_original_complete_policy_reference, prepare_original_policy_set_frame,
};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_complete_core_policy_patch_transaction,
    apply_original_policy_set_transaction,
};
use wf_types::identity::RequestId;

use super::super::TestResult;

pub(in crate::workflow_values) fn complete_policy(
    generation: u64,
    type_ttl: u64,
    state_ttl: u64,
) -> TestResult<CompletePolicyValue> {
    let profile = crate::fixtures::driver_limits().schema;
    let mut charge = PolicySchemaCharge::new(profile);
    let data = CompletePolicyData {
        workflow_type: b"r".to_vec(),
        version: None,
        max_active_ms: Some(5_000),
        retry: None,
        retention: Some(RetentionOverride {
            ttl_ms: Some(type_ttl),
            history_max_events: Some(30),
        }),
        states: BTreeMap::from([(
            b"q".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: Some(RetentionOverride {
                    ttl_ms: Some(state_ttl),
                    history_max_events: Some(40),
                }),
                governance: None,
            },
        )]),
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: None,
    };
    let candidate = restore_complete_policy(generation, data, profile, &mut charge)?;
    let value = prepare_complete_policy_value_v2(candidate, profile, &mut charge)?;
    verify_complete_policy_value_v2(&value, profile, &mut charge)?;
    Ok(value)
}

pub(in crate::workflow_values) fn original_policy(generation: u8) -> TestResult<StoredPolicyValue> {
    let mut raw = b"\x83\x68\x03\x77\x0eflow_policy_v1\x61".to_vec();
    raw.push(generation);
    raw.extend_from_slice(b"\x74\0\0\0\x04\x77\x04type\x6d\0\0\0\x01r");
    raw.extend_from_slice(b"\x77\x0dmax_active_ms\x62\0\0\x13\x88");
    raw.extend_from_slice(b"\x77\x09retention\x74\0\0\0\x02");
    raw.extend_from_slice(b"\x77\x12history_max_events\x61\x1e\x77\x06ttl_ms\x62\0\0\x2a\xf8");
    raw.extend_from_slice(b"\x77\x06states\x74\0\0\0\x01\x6d\0\0\0\x01q");
    raw.extend_from_slice(b"\x74\0\0\0\x01\x77\x09retention\x74\0\0\0\x02");
    raw.extend_from_slice(b"\x77\x12history_max_events\x61\x28\x77\x06ttl_ms\x62\0\0\x55\xf0");
    let schema = crate::fixtures::driver_limits().schema;
    let entry = decode_original_complete_policy_entry_portable(
        &raw,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?
    .ok_or("original input donor was not accepted")?;
    assert_eq!(entry.generation(), u64::from(generation));
    assert_eq!(entry.workflow_type(), Some(b"r".as_slice()));
    Ok(StoredPolicyValue::Original(raw))
}

pub(in crate::workflow_values) fn install_complete(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    patch: PolicyPatch,
    expected: Option<u64>,
    at: u64,
    now: u64,
) -> TestResult {
    let limits = crate::fixtures::driver_limits();
    let frame = prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            scope.clone(),
            RequestId::new(format!("complete-policy-{at}").into_bytes())?,
            crate::fixtures::clock(now)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: expected,
                patch,
            },
        ),
        limits.schema,
        limits.codec.admission(),
        &mut PolicySchemaCharge::new(limits.schema),
    )?;
    let result = apply_complete_core_policy_patch_transaction(
        store,
        scope,
        frame.as_bytes(),
        crate::fixtures::log(at),
        limits,
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("policy update did not retain a real result".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(())
}

pub(in crate::workflow_values) fn install_original(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    raw: &[u8],
    expected: Option<u64>,
    at: u64,
    now: u64,
) -> TestResult {
    let limits = crate::fixtures::driver_limits();
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let entry = decode_original_complete_policy_entry_portable(raw, limits.schema, &mut charge)?
        .ok_or("original fixture requires a valid donor")?;
    assert_eq!(
        raw,
        original_policy(u8::try_from(entry.generation())?)?.encoded(),
    );
    let frame = original_frame(scope, expected, at, now)?;
    let result = apply_original_policy_set_transaction(
        store,
        scope,
        frame.as_bytes(),
        crate::fixtures::log(at),
        limits,
        &mut charge,
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("original SET did not retain an actual result".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(())
}

fn original_frame(
    scope: &RequestScope,
    expected: Option<u64>,
    at: u64,
    now: u64,
) -> TestResult<PreparedBytes> {
    let limits = crate::fixtures::driver_limits();
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
    .map_err(|error| format!("literal original SET rejected: {error:?}"))?;
    let maps = [
        (0, [0, 1, 2].as_slice(), [0, 1, 2].as_slice()),
        (1, [0, 1].as_slice(), [0, 1].as_slice()),
        (2, [0].as_slice(), [0].as_slice()),
        (3, [0].as_slice(), [0].as_slice()),
        (4, [0, 1].as_slice(), [0, 1].as_slice()),
    ];
    let request = RequestId::new(format!("original-policy-{at}").into_bytes())?;
    let stamp = crate::fixtures::clock(now)?;
    let initial = OriginalPolicySetMapOrders::new([0; 32], None, &maps, &[], &[]);
    let input = OriginalPolicySetFrameInput::new(
        scope.clone(),
        request.clone(),
        stamp,
        &prepared,
        &initial,
    );
    let (bytes, _, _, range) = prepare_original_policy_set_frame_bytes(
        input,
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
        OriginalPolicySetFrameInput::new(scope.clone(), request, stamp, &prepared, &orders),
        limits.schema,
        limits.codec.admission(),
        &mut charge,
    )?)
}

pub(in crate::workflow_values) fn load_policy(
    store: &mut RedbStateStore,
    scope: &RequestScope,
) -> TestResult<Option<StoredPolicyValue>> {
    let limits = crate::fixtures::driver_limits();
    let tx = store.begin_apply(
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
    workflow_type: &[u8],
    policy: Option<&StoredPolicyValue>,
) -> TestResult<WorkflowValuePolicyWitness> {
    let schema = crate::fixtures::driver_limits().schema;
    let mut charge = PolicySchemaCharge::new(schema);
    let (profile, generation, digest) = match policy {
        None => {
            let reference = prepare_original_complete_policy_reference(
                OriginalPolicyReadValue::Absent,
                workflow_type,
                &canonical_original_etf_safety(),
                schema,
                &mut charge,
            )?;
            (0, reference.generation(), reference.digest())
        }
        Some(StoredPolicyValue::Complete(value)) => (
            1,
            value.generation(),
            PreparedDigest::new(Sha256::digest(value.encoded()).into()),
        ),
        Some(StoredPolicyValue::Original(raw)) => {
            let entry = decode_original_complete_policy_entry_portable(raw, schema, &mut charge)?
                .ok_or("original witness donor")?;
            (
                2,
                entry.generation(),
                PreparedDigest::new(Sha256::digest(raw).into()),
            )
        }
    };
    Ok(WorkflowValuePolicyWitness {
        workflow_type: workflow_type.to_vec(),
        profile,
        generation,
        digest,
    })
}
