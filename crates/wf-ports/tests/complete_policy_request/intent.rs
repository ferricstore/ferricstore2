//! Independent caller-intent vectors and unchanged legacy intent construction.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyReplaceInput, CompletePolicyRequestError,
    CompleteStatePolicyData, GovernanceData, PolicySchemaCharge, PolicySchemaError,
    prepare_complete_policy_replace_caller, restore_complete_policy,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RequestValueError};
use wf_ports::log::prepare_complete_policy_replace_intent;
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::{
    GUARDED_INTENT_DIGEST, MINIMAL_INTENT_BYTES, MINIMAL_INTENT_DIGEST, PRESENT_INTENT_BYTES,
    TestResult, input, minimal_data, present_data, profile,
};

mod variants;

#[test]
fn complete_replace_intent_has_fixed_minimal_bytes_and_digest() -> TestResult {
    let limits = profile(1_564)?;
    let admission = AdmissionLimits::new(512, 4_096, 7)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let request = input(minimal_data(), None)?;
    let owners = (
        request.data().workflow_type.as_ptr(),
        request.scope().tenant().as_bytes().as_ptr(),
        request.scope().group().as_bytes().as_ptr(),
        request.scope().recovery().as_bytes().as_ptr(),
        request.request().as_bytes().as_ptr(),
    );
    let result = prepare_complete_policy_replace_intent(request, limits, admission, &mut charge)?;
    let caller = result.caller();
    let actual = caller.input();
    assert!(actual.data() == &minimal_data());
    assert_eq!(actual.expected_generation(), None);
    assert_eq!(actual.scope().tenant().as_bytes(), b"t");
    assert_eq!(actual.scope().group().as_bytes(), b"g");
    assert_eq!(actual.scope().recovery().as_bytes(), b"r");
    assert_eq!(actual.request().as_bytes(), b"q");
    assert_eq!(
        actual.stamp(),
        CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
    );
    assert!(caller.schema_limits() == limits);
    assert_eq!(caller.admission_limits(), admission);
    assert_eq!(MINIMAL_INTENT_BYTES.len(), 52);
    let literal_digest: [u8; 32] = Sha256::digest(MINIMAL_INTENT_BYTES).into();
    assert_eq!(literal_digest, MINIMAL_INTENT_DIGEST);
    assert_eq!(result.digest(), &MINIMAL_INTENT_DIGEST);
    assert_eq!((charge.g(), charge.p(), charge.total()), (1_564, 0, 1_564));
    // Addresses observe moved sources; they do not measure native allocations.
    assert_eq!(
        (
            actual.data().workflow_type.as_ptr(),
            actual.scope().tenant().as_bytes().as_ptr(),
            actual.scope().group().as_bytes().as_ptr(),
            actual.scope().recovery().as_bytes().as_ptr(),
            actual.request().as_bytes().as_ptr(),
        ),
        owners
    );

    assert_supplied_empty_groups()
}

fn assert_supplied_empty_groups() -> TestResult {
    // An independent input preserves supplied empty groups rather than None.
    let empty_limits = profile(100_000)?;
    let mut empty_charge = PolicySchemaCharge::new(empty_limits);
    let mut value = minimal_data();
    value.retry = Some(RetryOverride::default());
    value.retention = Some(RetentionOverride::default());
    value.governance = Some(GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: None,
        budgets: None,
        circuits: None,
        approvals: None,
    });
    let result = prepare_complete_policy_replace_intent(
        input(value, None)?,
        empty_limits,
        AdmissionLimits::default(),
        &mut empty_charge,
    )?;
    let mut literal = MINIMAL_INTENT_BYTES[..43].to_vec();
    literal.extend_from_slice(&[1, 0, 0, 0, 1, 0, 0]);
    literal.extend_from_slice(&[0; 6]);
    literal.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    let expected: [u8; 32] = Sha256::digest(&literal).into();
    assert_eq!(literal.len(), 64);
    assert_eq!(result.digest(), &expected);
    let actual = result.caller().input().data();
    assert_eq!(actual.retry.as_ref(), Some(&RetryOverride::default()));
    assert_eq!(actual.retention, Some(RetentionOverride::default()));
    let governance = actual
        .governance
        .as_ref()
        .ok_or("missing empty governance")?;
    assert!(governance.mode.is_none() && governance.effects.is_none());
    assert!(governance.audit.is_none() && governance.limits.is_none());
    assert!(governance.budgets.is_none() && governance.circuits.is_none());
    assert!(governance.approvals.is_none());
    assert!(actual.workflow_type == b"r" && actual.states.is_empty());
    assert!(actual.version.is_none() && actual.max_active_ms.is_none());
    assert!(actual.indexed_attributes.is_none() && actual.indexed_state_meta.is_none());
    assert_eq!(
        CompletePolicyRequestError::ScopeLimit.to_string(),
        "complete policy request scope limit exceeded"
    );
    assert_eq!(
        CompletePolicyRequestError::IntentMismatch.to_string(),
        "complete policy request intent mismatch"
    );
    Ok(())
}

#[test]
fn complete_replace_intent_preserves_every_field_presence_and_order() -> TestResult {
    let limits = profile(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let value = present_data();
    let retry = value.retry.as_ref().ok_or("missing retry")?;
    let target = retry.exhausted_to.as_ref().ok_or("missing target")?;
    let key = value.states.first_key_value().ok_or("missing state")?.0;
    let attributes = value
        .indexed_attributes
        .as_ref()
        .ok_or("missing attributes")?;
    let governance = value.governance.as_ref().ok_or("missing governance")?;
    let circuits = governance.circuits.as_ref().ok_or("missing circuits")?;
    let circuit = circuits.get(&b"c"[..]).ok_or("missing circuit")?;
    let classes = circuit.error_classes.as_ref().ok_or("missing classes")?;
    let budgets = governance.budgets.as_ref().ok_or("missing budgets")?;
    let budget = budgets.get(&b"b"[..]).ok_or("missing budget")?;
    let unit = budget.unit.as_ref().ok_or("missing unit")?;
    let owners = (
        value.workflow_type.as_ptr(),
        target.as_ptr(),
        key.as_ptr(),
        attributes.as_ptr(),
        classes.as_ptr(),
        unit.as_ptr(),
    );
    let result = prepare_complete_policy_replace_intent(
        input(value, None)?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let actual = result.caller().input().data();
    // Complete equality covers all nine fields, every nested scalar and Option.
    assert!(actual == &present_data());
    assert_eq!(PRESENT_INTENT_BYTES.len(), 394);
    let expected: [u8; 32] = Sha256::digest(PRESENT_INTENT_BYTES).into();
    assert_eq!(result.digest(), &expected);
    let retry = actual.retry.as_ref().ok_or("missing moved retry")?;
    let target = retry.exhausted_to.as_ref().ok_or("missing moved target")?;
    let key = actual
        .states
        .first_key_value()
        .ok_or("missing moved state")?
        .0;
    let attributes = actual
        .indexed_attributes
        .as_ref()
        .ok_or("missing moved attributes")?;
    let governance = actual
        .governance
        .as_ref()
        .ok_or("missing moved governance")?;
    let circuits = governance
        .circuits
        .as_ref()
        .ok_or("missing moved circuits")?;
    let circuit = circuits.get(&b"c"[..]).ok_or("missing moved circuit")?;
    let classes = circuit
        .error_classes
        .as_ref()
        .ok_or("missing moved classes")?;
    let budgets = governance.budgets.as_ref().ok_or("missing moved budgets")?;
    let budget = budgets.get(&b"b"[..]).ok_or("missing moved budget")?;
    let unit = budget.unit.as_ref().ok_or("missing moved unit")?;
    assert_eq!(
        (
            actual.workflow_type.as_ptr(),
            target.as_ptr(),
            key.as_ptr(),
            attributes.as_ptr(),
            classes.as_ptr(),
            unit.as_ptr(),
        ),
        owners
    );
    check_complete_replace_prior_work(&mut charge)?;
    Ok(())
}

#[test]
fn complete_replace_intent_binds_tenant_guard_and_excludes_preparation_fields() -> TestResult {
    let limits = profile(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let guarded = prepare_complete_policy_replace_intent(
        input(minimal_data(), Some(0))?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(guarded.digest(), &GUARDED_INTENT_DIGEST);
    assert_eq!(guarded.caller().input().expected_generation(), Some(0));
    assert!(guarded.caller().input().data() == &minimal_data());
    assert_ne!(guarded.digest(), &MINIMAL_INTENT_DIGEST);

    let request = CompletePolicyReplaceInput::new(
        RequestScope::new(
            TenantId::new(b"u".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"r".to_vec())?,
        ),
        RequestId::new(b"q".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
        None,
        minimal_data(),
    );
    let changed = prepare_complete_policy_replace_intent(
        request,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let mut literal = MINIMAL_INTENT_BYTES.to_vec();
    literal[34] = b'u';
    let expected: [u8; 32] = Sha256::digest(&literal).into();
    assert_eq!(changed.digest(), &expected);
    assert_ne!(changed.digest(), &MINIMAL_INTENT_DIGEST);
    assert_eq!(changed.caller().input().scope().tenant().as_bytes(), b"u");

    let stamp = CommittedTime::new(HlcTimestamp::new(99, 7)?, 98)?;
    let request = CompletePolicyReplaceInput::new(
        RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"other-group".to_vec())?,
            RecoveryId::new(b"other-recovery".to_vec())?,
        ),
        RequestId::new(b"other-request".to_vec())?,
        stamp,
        None,
        minimal_data(),
    );
    let unchanged = prepare_complete_policy_replace_intent(
        request,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(unchanged.digest(), &MINIMAL_INTENT_DIGEST);
    let actual = unchanged.caller().input();
    assert!(actual.data() == &minimal_data());
    assert_eq!(actual.scope().group().as_bytes(), b"other-group");
    assert_eq!(actual.scope().recovery().as_bytes(), b"other-recovery");
    assert_eq!(actual.request().as_bytes(), b"other-request");
    assert_eq!(actual.stamp(), stamp);

    // Representable business-invalid CAS waits for M1, not caller normalization.
    let guarded = prepare_complete_policy_replace_intent(
        input(minimal_data(), Some(u64::MAX))?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let mut literal = MINIMAL_INTENT_BYTES[..35].to_vec();
    literal.push(1);
    literal.extend_from_slice(&u64::MAX.to_be_bytes());
    literal.extend_from_slice(&MINIMAL_INTENT_BYTES[36..]);
    let expected: [u8; 32] = Sha256::digest(&literal).into();
    assert_eq!(guarded.digest(), &expected);
    assert_eq!(
        guarded.caller().input().expected_generation(),
        Some(u64::MAX)
    );
    Ok(())
}

#[test]
fn complete_replace_intent_keeps_legacy_version_one_construction() -> TestResult {
    let legacy = ClientIntent::new(1, MINIMAL_INTENT_DIGEST)?;
    assert_eq!(legacy.version(), 1);
    assert_eq!(legacy.digest(), &MINIMAL_INTENT_DIGEST);
    assert_eq!(
        ClientIntent::new(0, MINIMAL_INTENT_DIGEST),
        Err(RequestValueError::UnsupportedVersion)
    );
    assert_eq!(
        ClientIntent::new(2, MINIMAL_INTENT_DIGEST),
        Err(RequestValueError::UnsupportedVersion)
    );
    let limits = profile(1_564)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = prepare_complete_policy_replace_intent(
        input(minimal_data(), None)?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(result.digest(), legacy.digest());
    assert!(result.caller().input().data() == &minimal_data());
    assert_eq!(
        ClientIntent::new(2, *result.digest()),
        Err(RequestValueError::UnsupportedVersion)
    );
    let schema = PolicySchemaError::WorkBytes {
        bytes: 1_564,
        maximum: 1_563,
    };
    assert_eq!(
        CompletePolicyRequestError::from(schema),
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(schema))
    );
    assert_eq!(
        CompletePolicyRequestError::from(CompletePolicyEncodingError::ByteLimit),
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );

    // Independent V2 field order remains separate from the V1 registry.
    let ordered_limits = profile(100_000)?;
    let mut ordered_charge = PolicySchemaCharge::new(ordered_limits);
    let mut value = minimal_data();
    value.states = BTreeMap::from([
        (
            vec![0xff],
            CompleteStatePolicyData {
                mode: Some(StateMode::Fifo),
                retry: None,
                retention: None,
                governance: None,
            },
        ),
        (
            vec![0],
            CompleteStatePolicyData {
                mode: Some(StateMode::Parallel),
                retry: None,
                retention: None,
                governance: None,
            },
        ),
    ]);
    value.indexed_attributes = Some(vec![b"z".to_vec(), b"a".to_vec()]);
    let result = prepare_complete_policy_replace_intent(
        input(value, None)?,
        ordered_limits,
        AdmissionLimits::default(),
        &mut ordered_charge,
    )?;
    let mut literal = MINIMAL_INTENT_BYTES[..45].to_vec();
    literal.extend_from_slice(&[
        0, 0, 0, 2, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0xff, 1, 1, 0, 0, 0, 1, 0, 0, 0, 2,
        0, 0, 0, 1, b'z', 0, 0, 0, 1, b'a', 0, 0,
    ]);
    let expected: [u8; 32] = Sha256::digest(&literal).into();
    assert_eq!(result.digest(), &expected);
    let actual = result.caller().input().data();
    let first = actual
        .states
        .first_key_value()
        .ok_or("missing ordered state")?;
    let last = actual
        .states
        .last_key_value()
        .ok_or("missing ordered state")?;
    assert_eq!(first.0, &[0]);
    assert_eq!(last.0, &[0xff]);
    assert_eq!(first.1.mode, Some(StateMode::Parallel));
    assert_eq!(last.1.mode, Some(StateMode::Fifo));
    assert_eq!(
        actual.indexed_attributes.as_deref(),
        Some([b"z".to_vec(), b"a".to_vec()].as_slice())
    );
    Ok(())
}

fn check_complete_replace_prior_work(charge: &mut PolicySchemaCharge) -> TestResult {
    // Prior TRUE restoration is paid; caller measurement never resets its peak.
    let scratch_limits = profile(3_556)?;
    let mut prior_charge = PolicySchemaCharge::new(scratch_limits);
    let mut value = minimal_data();
    value.indexed_attributes = Some(vec![b"a".to_vec()]);
    let retained = restore_complete_policy(1, value, scratch_limits, &mut prior_charge)?;
    assert_eq!(
        (prior_charge.g(), prior_charge.p(), prior_charge.total()),
        (1_928, 64, 1_992)
    );
    let caller = prepare_complete_policy_replace_caller(
        input(minimal_data(), None)?,
        scratch_limits,
        AdmissionLimits::default(),
        &mut prior_charge,
    )?;
    assert!(caller.input().data() == &minimal_data());
    assert_eq!(
        retained.data().indexed_attributes.as_deref(),
        Some([b"a".to_vec()].as_slice())
    );
    assert_eq!(
        (prior_charge.g(), prior_charge.p(), prior_charge.total()),
        (3_492, 64, 3_556)
    );

    let before = (charge.g(), charge.p());
    let mut value = minimal_data();
    value.workflow_type = vec![b'r'; 262_145];
    let error = prepare_complete_policy_replace_caller(
        input(value, None)?,
        profile(1_000_001)?,
        AdmissionLimits::default(),
        charge,
    )
    .err()
    .ok_or("missing profile refusal")?;
    assert_eq!(
        error,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p()), before);
    Ok(())
}
