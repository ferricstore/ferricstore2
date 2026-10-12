//! Shared replacement value, current-authority, and accounting observations.

use std::collections::BTreeMap;

use wf_domain::policy::complete::CompletePolicyEncodingError::{EncodingMismatch, Schema};
use wf_domain::policy::complete::PolicySchemaError::{InvalidAuthority, Policy, WorkBytes};
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, CompletePolicyData, CompletePolicyInstallPlan, CompletePolicyValue,
    DenialAuditMode, PolicySchemaCharge, PolicySchemaLimits, PreparedCompletePolicyReplace,
    decide_complete_policy_replace, prepare_complete_policy_replace,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::policy::{Backoff, BackoffKind};
use wf_domain::retention::RetentionOverride;

use super::super::{TestResult, bind, candidate, data, limits};

/// Checks all supplied empty, false, and zero replacement values.
pub(super) fn assert_present_replacement(value: &CompletePolicyData) -> TestResult {
    assert_eq!(value.workflow_type, b"r");
    assert!(value.version.is_none() && value.max_active_ms.is_none() && value.states.is_empty());
    assert_eq!(value.retention, Some(RetentionOverride::default()));
    assert_eq!(
        value.indexed_attributes.as_deref(),
        Some(<&[Vec<u8>]>::default())
    );
    assert!(value.indexed_state_meta.is_none());
    let retry = value.retry.as_ref().ok_or("missing retry")?;
    assert_eq!(retry.max_retries, Some(0));
    assert_eq!(
        retry.backoff,
        Some(Backoff {
            kind: BackoffKind::None,
            base_ms: 0,
            max_ms: 0,
            jitter_pct: 0,
        })
    );
    assert!(retry.exhausted_to.is_none());
    let governance = value.governance.as_ref().ok_or("missing governance")?;
    assert!(governance.mode.is_none());
    let effects = governance.effects.as_ref().ok_or("missing effects")?;
    assert_eq!(
        effects.allowed.as_deref(),
        Some([b"z".to_vec(), b"a".to_vec()].as_slice())
    );
    assert_eq!(effects.denied.as_deref(), Some(<&[Vec<u8>]>::default()));
    assert_eq!(effects.require_idempotency_key, Some(false));
    assert!(
        governance.audit
            == Some(AuditData {
                effect_events: Some(false),
                denials: Some(DenialAuditMode::Off),
            })
    );
    assert!(
        governance
            .limits
            .as_ref()
            .ok_or("missing limits")?
            .is_empty()
    );
    assert!(
        governance
            .budgets
            .as_ref()
            .ok_or("missing budgets")?
            .is_empty()
    );
    assert!(
        governance
            .circuits
            .as_ref()
            .ok_or("missing circuits")?
            .is_empty()
    );
    assert!(
        governance.approvals.as_ref()
            == Some(&ApprovalsData {
                effects: Some(Vec::new()),
                states: Some(Vec::new()),
                assignees: Some(Vec::new()),
                timeout_ms: None,
            })
    );
    Ok(())
}

/// Returns a plan whose lifetime belongs only to the prepared value.
pub(super) fn observe_current_replace_plan<'a>(
    original: &PreparedCompletePolicyReplace,
    prepared: &'a PreparedCompletePolicyReplace,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyInstallPlan<'a>> {
    let corrupt = bind(
        candidate(
            1,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            charge,
        )?,
        vec![0; 40],
        0..22,
        profile,
        charge,
    )?;
    let original_bytes = corrupt.encoded().to_vec();
    for (existing, migration, query, expected) in [
        (
            Some(original.value()),
            0,
            b"foreign".as_slice(),
            Schema(InvalidAuthority),
        ),
        (Some(&corrupt), 0, b"r".as_slice(), EncodingMismatch),
        (
            Some(original.value()),
            7,
            b"r".as_slice(),
            Schema(Policy(PolicyError::PreparedMismatch)),
        ),
    ] {
        let result =
            decide_complete_policy_replace(existing, migration, query, prepared, profile, charge);
        assert!(matches!(result, Err(error) if error == expected));
    }
    assert_eq!(corrupt.generation(), 1);
    assert!(corrupt.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    assert_eq!(corrupt.encoded(), original_bytes);
    let plan = {
        let query = b"r".to_vec();
        let current = prepare_complete_policy_replace(
            None,
            0,
            None,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            charge,
        )?;
        let mut local_charge = PolicySchemaCharge::new(profile);
        decide_complete_policy_replace(
            Some(current.value()),
            0,
            &query,
            prepared,
            profile,
            &mut local_charge,
        )?
    };
    Ok(plan)
}

/// Retains peak scratch and refuses an unadmitted scalar decision.
pub(super) fn assert_replacement_scratch_and_scalar_refusal() -> TestResult {
    let profile = limits(20_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let mut input = data(b"r".to_vec(), None, BTreeMap::new());
    input.indexed_attributes = Some(vec![b"a".to_vec()]);
    let held = candidate(1, input, profile, &mut charge)?;
    assert_eq!((charge.g(), charge.p()), (1_928, 64));
    let prepared = prepare_complete_policy_replace(
        None,
        0,
        None,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert_eq!(
        held.data().indexed_attributes.as_deref(),
        Some([b"a".to_vec()].as_slice())
    );
    assert!(prepared.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    assert_eq!((charge.g(), charge.p()), (4_612, 64));
    let profile = limits(127)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let result = prepare_complete_policy_replace(
        None,
        0,
        None,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    );
    assert!(matches!(
        result,
        Err(Schema(WorkBytes {
            bytes: 128,
            maximum: 127
        }))
    ));
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}

/// Checks decision priority on the same charge and original authorities.
pub(super) fn assert_decide_replace_priority(
    current: &PreparedCompletePolicyReplace,
    corrupt: &CompletePolicyValue,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stale = prepare_complete_policy_replace(
        None,
        0,
        Some(0),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        charge,
    )?;
    let before = (charge.g(), charge.p());
    let result = decide_complete_policy_replace(
        Some(corrupt),
        MAX_POLICY_GENERATION + 1,
        b"foreign",
        &stale,
        profile,
        charge,
    );
    assert!(matches!(
        result,
        Err(Schema(Policy(PolicyError::StaleGeneration)))
    ));
    assert_eq!((charge.g(), charge.p()), (before.0 + 128, before.1));
    let next = prepare_complete_policy_replace(
        Some(current.value()),
        0,
        Some(1),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        charge,
    )?;
    for (existing, query, wanted) in [
        (Some(corrupt), b"r".as_slice(), EncodingMismatch),
        (
            Some(current.value()),
            b"foreign".as_slice(),
            Schema(InvalidAuthority),
        ),
        (
            Some(current.value()),
            b"r".as_slice(),
            Schema(InvalidAuthority),
        ),
    ] {
        let result = decide_complete_policy_replace(
            existing,
            MAX_POLICY_GENERATION + 1,
            query,
            &next,
            profile,
            charge,
        );
        assert!(matches!(result, Err(error) if error == wanted));
    }
    Ok(())
}
