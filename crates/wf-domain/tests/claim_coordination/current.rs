//! Original normalized callers resolve full backing, never a supplied projection.
use super::{TestResult, caller};
use std::collections::BTreeMap;
use wf_domain::claim::caller::ClaimDueInput;
use wf_domain::claim::coordination::{
    ClaimCoordinationError, ClaimCoordinationLimits, CurrentRunningCreditError as Error,
    resolve_current_running_credit as resolve,
};
use wf_domain::policy::complete::{
    CompletePolicyData, GovernanceData, LimitData, PolicySchemaCharge, PolicySchemaLimits,
    StoredPolicyValue, prepare_complete_policy_value_v2, restore_complete_policy,
};
use wf_domain::policy::complete::{
    RunningClaimPolicyVersion, encode_original_policy_from_complete,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::workflow::active::LimitEnforcement as Enforcement;
use wf_types::version::TimestampMillis;

fn stored() -> TestResult<StoredPolicyValue> {
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let data = CompletePolicyData {
        workflow_type: b"job".to_vec(),
        version: Some(PolicyVersion::Bytes(b"v\0x".to_vec())),
        max_active_ms: None,
        retry: None,
        retention: None,
        states: BTreeMap::new(),
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            budgets: None,
            circuits: None,
            approvals: None,
            limits: Some(BTreeMap::from([(
                b"running".to_vec(),
                LimitData {
                    limit: 8,
                    enforcement: Some(Enforcement::StrictGlobal),
                    lease_size: Some(9),
                },
            )])),
        }),
    };
    let value = restore_complete_policy(7, data, schema, &mut charge)?;
    Ok(StoredPolicyValue::Complete(Box::new(
        prepare_complete_policy_value_v2(value, schema, &mut charge)?,
    )))
}

#[test]
fn actual_complete_policy_drives_original_demand_and_trusted_time() -> TestResult {
    let value = stored()?;
    let mut input = ClaimDueInput::new(b"job", b"worker");
    input.limit = Some(2);
    input.lease_ms = Some(7);
    input.requested_now_ms = Some(u64::MAX);
    let call = caller(input)?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let demand = resolve(
        &call,
        Some(&value),
        TimestampMillis::new(100),
        4,
        schema,
        &mut charge,
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("demand")?;
    assert_eq!(
        (
            demand.amount(),
            demand.limit(),
            demand.lease_size(),
            demand.ttl_ms()
        ),
        (2, 8, 9, 7)
    );
    assert!(demand.auto_lease());
    assert!(demand.enforcement() == Enforcement::StrictGlobal);
    assert_eq!(demand.config_version(), Some(7));
    assert_eq!(demand.trusted_now().get(), 100);
    let StoredPolicyValue::Complete(typed) = &value else {
        return Err("typed owner".into());
    };
    let Some(PolicyVersion::Bytes(label)) = &typed.data().version else {
        return Err("label".into());
    };
    let Some(RunningClaimPolicyVersion::Bytes(actual)) = demand.policy_version() else {
        return Err("borrowed version".into());
    };
    assert_eq!(actual.as_ptr(), label.as_ptr());
    assert!(charge.g() > 0);
    Ok(())
}

#[test]
fn raw_demand_keeps_original_version_owner_and_omits_unconfigured_label() -> TestResult {
    let value = stored()?;
    let StoredPolicyValue::Complete(typed) = value else {
        return Err("typed owner".into());
    };
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let encoded = encode_original_policy_from_complete(&typed, schema, &mut charge, 0, 262_144)?;
    let generation = 3 + 2 + "flow_policy_v1".len();
    assert_eq!(&encoded[generation..generation + 2], &[97, 7]);
    let call = caller(ClaimDueInput::new(b"job", b"worker"))?;
    for number in [0, 7] {
        let mut raw = encoded.clone();
        raw[generation + 1] = number;
        let offset = raw
            .windows(3)
            .position(|bytes| bytes == b"v\0x")
            .ok_or("label offset")?;
        let stored = StoredPolicyValue::Original(raw);
        let demand = resolve(
            &call,
            Some(&stored),
            TimestampMillis::new(100),
            4,
            schema,
            &mut charge,
            ClaimCoordinationLimits::default(),
        )?
        .ok_or("raw demand")?;
        assert!(demand.auto_lease());
        if number == 0 {
            assert!(demand.config_version().is_none());
            assert!(demand.policy_version().is_none());
        } else {
            assert_eq!(demand.config_version(), Some(7));
            let Some(RunningClaimPolicyVersion::Bytes(label)) = demand.policy_version() else {
                return Err("raw label".into());
            };
            assert_eq!(label.as_ptr(), stored.encoded()[offset..].as_ptr());
        }
    }
    Ok(())
}

#[test]
fn valid_explicit_pair_skips_unused_corrupt_policy_and_schema() -> TestResult {
    let value = StoredPolicyValue::Original(vec![0xff]);
    let mut input = ClaimDueInput::new(b"job", b"worker");
    input.governance_limit_scope = Some(b"scope\0");
    input.governance_shard_id = Some(2);
    input.limit = Some(2);
    input.lease_ms = Some(7);
    let call = caller(input)?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let demand = resolve(
        &call,
        Some(&value),
        TimestampMillis::new(100),
        4,
        schema,
        &mut charge,
        ClaimCoordinationLimits::default(),
    )?
    .ok_or("explicit demand")?;
    assert_eq!(
        (
            demand.amount(),
            demand.limit(),
            demand.lease_size(),
            demand.ttl_ms()
        ),
        (2, 2, 2, 7)
    );
    assert!(!demand.auto_lease());
    assert!(demand.enforcement() == Enforcement::ApproximateGlobal);
    assert!(demand.config_version().is_none());
    assert_eq!(charge.total(), 0);
    Ok(())
}

#[test]
fn invalid_pair_precedes_corrupt_policy_without_charging_or_fallback() -> TestResult {
    let value = StoredPolicyValue::Original(vec![0xff]);
    let mut input = ClaimDueInput::new(b"job", b"worker");
    input.governance_limit_scope = Some(b"scope");
    let call = caller(input)?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    assert!(matches!(
        resolve(
            &call,
            Some(&value),
            TimestampMillis::new(100),
            4,
            schema,
            &mut charge,
            ClaimCoordinationLimits::default()
        ),
        Err(Error::Coordination(
            ClaimCoordinationError::InvalidGovernancePair
        ))
    ));
    assert_eq!(charge.total(), 0);
    Ok(())
}

#[test]
fn absent_current_policy_is_ungoverned_after_caller_admission() -> TestResult {
    let call = caller(ClaimDueInput::new(b"job", b"worker"))?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    assert!(
        resolve(
            &call,
            None,
            TimestampMillis::new(100),
            4,
            schema,
            &mut charge,
            ClaimCoordinationLimits::default()
        )?
        .is_none()
    );
    assert_eq!(charge.total(), 0);
    Ok(())
}
