//! Normalized replacement generation, ownership, and same-account decisions.

use std::collections::BTreeMap;

use wf_domain::policy::complete::CompletePolicyEncodingError::{EncodingMismatch, Schema};
use wf_domain::policy::complete::PolicySchemaError::{
    InvalidAuthority, Policy, ProfileMismatch, WorkBytes,
};
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, CompleteStatePolicyData, DenialAuditMode, EffectsData,
    GovernanceData, GovernanceMode, PolicySchemaCharge, decide_complete_policy_replace,
    prepare_complete_policy_replace,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;

use super::{TestResult, bind, candidate, data, limits};

mod assertions;

use assertions::{
    assert_decide_replace_priority, assert_present_replacement,
    assert_replacement_scratch_and_scalar_refusal, observe_current_replace_plan,
};

#[test]
fn complete_replace_allocates_stored_or_migration_generation() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let first = prepare_complete_policy_replace(
        None,
        0,
        Some(0),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert_eq!(first.expected_generation(), Some(0));
    assert_eq!(first.value().generation(), 1);
    assert!(first.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    for (existing, migration, expected, assigned) in [
        (Some(first.value()), 0, None, 2),
        (Some(first.value()), 7, Some(1), 8),
        (None, 7, Some(0), 8),
        (None, MAX_POLICY_GENERATION - 1, None, MAX_POLICY_GENERATION),
    ] {
        let prepared = prepare_complete_policy_replace(
            existing,
            migration,
            expected,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        )?;
        assert_eq!(prepared.expected_generation(), expected);
        assert_eq!(prepared.value().generation(), assigned);
        assert!(prepared.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        let mut literal = vec![
            70, 87, 80, 86, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 22, 70, 87, 80, 83, 0, 2, 0, 0,
            0, 1, b'r', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        literal[6..14].copy_from_slice(&assigned.to_be_bytes());
        assert_eq!(prepared.value().encoded(), literal);
    }
    let maximum = prepare_complete_policy_replace(
        None,
        MAX_POLICY_GENERATION - 1,
        None,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    for (existing, migration, expected) in [
        (Some(maximum.value()), 0, Some(MAX_POLICY_GENERATION)),
        (None, MAX_POLICY_GENERATION, None),
    ] {
        let result = prepare_complete_policy_replace(
            existing,
            migration,
            expected,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        );
        assert!(matches!(
            result,
            Err(Schema(Policy(PolicyError::GenerationExhausted)))
        ));
    }
    let result = prepare_complete_policy_replace(
        None,
        0,
        Some(MAX_POLICY_GENERATION + 1),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    );
    assert!(matches!(
        result,
        Err(Schema(Policy(PolicyError::InvalidGeneration)))
    ));
    let result = prepare_complete_policy_replace(
        None,
        0,
        Some(1),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    );
    assert!(matches!(
        result,
        Err(Schema(Policy(PolicyError::StaleGeneration)))
    ));
    Ok(())
}

#[test]
fn complete_replace_cas_preserves_existing_policy() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let states = BTreeMap::from([(
        b"queued".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Fifo),
            retry: Some(RetryOverride::default()),
            retention: Some(RetentionOverride::default()),
            governance: None,
        },
    )]);
    let mut input = data(b"job".to_vec(), Some(PolicyVersion::Number(7)), states);
    input.max_active_ms = Some(1_000);
    input.retry = Some(RetryOverride::default());
    input.retention = Some(RetentionOverride::default());
    input.indexed_attributes = Some(vec![b"owner".to_vec()]);
    input.indexed_state_meta = Some(b"status".to_vec());
    let old = prepare_complete_policy_replace(None, 0, None, input, profile, &mut charge)?;
    let old_bytes = old.value().encoded().to_vec();
    let result = prepare_complete_policy_replace(
        Some(old.value()),
        0,
        Some(0),
        data(b"job".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    );
    assert!(matches!(
        result,
        Err(Schema(Policy(PolicyError::StaleGeneration)))
    ));
    let states = BTreeMap::from([(
        b"queued".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Fifo),
            retry: Some(RetryOverride::default()),
            retention: Some(RetentionOverride::default()),
            governance: None,
        },
    )]);
    let mut expected = data(b"job".to_vec(), Some(PolicyVersion::Number(7)), states);
    expected.max_active_ms = Some(1_000);
    expected.retry = Some(RetryOverride::default());
    expected.retention = Some(RetentionOverride::default());
    expected.indexed_attributes = Some(vec![b"owner".to_vec()]);
    expected.indexed_state_meta = Some(b"status".to_vec());
    assert!(old.value().data() == &expected);
    assert_eq!(old.value().generation(), 1);
    assert_eq!(old.value().encoded(), old_bytes);
    let next = prepare_complete_policy_replace(
        Some(old.value()),
        5,
        None,
        data(b"job".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    let plan =
        decide_complete_policy_replace(Some(old.value()), 5, b"job", &next, profile, &mut charge)?;
    assert_eq!(next.expected_generation(), None);
    assert_eq!(next.value().generation(), 6);
    assert_eq!(plan.expected_generation(), 1);
    assert_eq!(plan.value().generation(), 6);
    assert!(plan.value().data() == &data(b"job".to_vec(), None, BTreeMap::new()));
    assert_eq!(plan.value().encoded(), next.value().encoded());
    assert!(old.value().data() == &expected);
    assert_eq!(old.value().encoded(), old_bytes);
    assert!(std::ptr::eq(plan.value(), next.value()));
    Ok(())
}

#[test]
fn complete_replace_resets_omissions_and_retains_present_empty() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let states = BTreeMap::from([(
        b"old".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Fifo),
            retry: None,
            retention: None,
            governance: None,
        },
    )]);
    let mut input = data(
        b"r".to_vec(),
        Some(PolicyVersion::Bytes(b"old".to_vec())),
        states,
    );
    input.max_active_ms = Some(1_000);
    input.retry = Some(RetryOverride::default());
    input.retention = Some(RetentionOverride::default());
    input.indexed_attributes = Some(vec![b"owner".to_vec()]);
    input.indexed_state_meta = Some(b"version".to_vec());
    input.governance = Some(GovernanceData {
        mode: Some(GovernanceMode::Full),
        effects: None,
        audit: None,
        limits: None,
        budgets: None,
        circuits: None,
        approvals: None,
    });
    let old = prepare_complete_policy_replace(None, 0, None, input, profile, &mut charge)?;
    let cleared = prepare_complete_policy_replace(
        Some(old.value()),
        0,
        Some(1),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert_eq!(cleared.value().generation(), 2);
    assert!(cleared.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    let mut input = data(b"r".to_vec(), None, BTreeMap::new());
    input.retry = Some(RetryOverride {
        max_retries: Some(0),
        backoff: Some(Backoff {
            kind: BackoffKind::None,
            base_ms: 0,
            max_ms: 0,
            jitter_pct: 0,
        }),
        exhausted_to: None,
    });
    input.retention = Some(RetentionOverride::default());
    input.indexed_attributes = Some(Vec::new());
    input.governance = Some(GovernanceData {
        mode: None,
        effects: Some(EffectsData {
            allowed: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            denied: Some(Vec::new()),
            require_idempotency_key: Some(false),
        }),
        audit: Some(AuditData {
            effect_events: Some(false),
            denials: Some(DenialAuditMode::Off),
        }),
        limits: Some(BTreeMap::new()),
        budgets: Some(BTreeMap::new()),
        circuits: Some(BTreeMap::new()),
        approvals: Some(ApprovalsData {
            effects: Some(Vec::new()),
            states: Some(Vec::new()),
            assignees: Some(Vec::new()),
            timeout_ms: None,
        }),
    });
    let present =
        prepare_complete_policy_replace(Some(old.value()), 0, None, input, profile, &mut charge)?;
    let value = present.value().data();
    assert_eq!(present.value().generation(), 2);
    assert_present_replacement(value)?;
    Ok(())
}

#[test]
fn complete_replace_rechecks_current_type_bytes_and_generation() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let old = prepare_complete_policy_replace(
        None,
        0,
        None,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    let mut input = data(
        b"r".to_vec(),
        Some(PolicyVersion::Bytes(b"label".to_vec())),
        BTreeMap::new(),
    );
    input.indexed_attributes = Some(vec![b"z".to_vec(), b"a".to_vec()]);
    input.indexed_state_meta = Some(b"status".to_vec());
    let Some(PolicyVersion::Bytes(version)) = input.version.as_ref() else {
        return Err("missing version".into());
    };
    let attributes = input
        .indexed_attributes
        .as_ref()
        .ok_or("missing attributes")?;
    let owners = (
        input.workflow_type.as_ptr(),
        version.as_ptr(),
        attributes.as_ptr(),
        attributes[0].as_ptr(),
        attributes[1].as_ptr(),
        input
            .indexed_state_meta
            .as_ref()
            .ok_or("missing metadata")?
            .as_ptr(),
    );
    let prepared = prepare_complete_policy_replace(
        Some(old.value()),
        0,
        Some(1),
        input,
        profile,
        &mut charge,
    )?;
    let plan = observe_current_replace_plan(&old, &prepared, profile, &mut charge)?;
    let mut expected = data(
        b"r".to_vec(),
        Some(PolicyVersion::Bytes(b"label".to_vec())),
        BTreeMap::new(),
    );
    expected.indexed_attributes = Some(vec![b"z".to_vec(), b"a".to_vec()]);
    expected.indexed_state_meta = Some(b"status".to_vec());
    assert_eq!(prepared.expected_generation(), Some(1));
    assert_eq!(plan.expected_generation(), 1);
    assert_eq!(plan.value().generation(), 2);
    assert!(prepared.value().data() == &expected);
    assert!(plan.value().data() == &expected);
    assert_eq!(plan.value().encoded(), prepared.value().encoded());
    let moved = prepared.value().data();
    let Some(PolicyVersion::Bytes(version)) = moved.version.as_ref() else {
        return Err("missing moved version".into());
    };
    let attributes = moved
        .indexed_attributes
        .as_ref()
        .ok_or("missing moved attributes")?;
    assert!(std::ptr::eq(plan.value(), prepared.value()));
    assert_eq!(
        (
            moved.workflow_type.as_ptr(),
            version.as_ptr(),
            attributes.as_ptr(),
            attributes[0].as_ptr(),
            attributes[1].as_ptr(),
            moved
                .indexed_state_meta
                .as_ref()
                .ok_or("missing moved metadata")?
                .as_ptr(),
        ),
        owners
    );
    Ok(())
}

#[test]
fn complete_replace_keeps_one_nonrollback_schema_account() -> TestResult {
    for maximum in [13_167, 13_166] {
        assert_nonrollback_account(maximum)?;
    }
    assert_replacement_scratch_and_scalar_refusal()?;
    Ok(())
}

fn assert_nonrollback_account(maximum: u64) -> TestResult {
    let profile = limits(maximum)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let first = prepare_complete_policy_replace(
        None,
        0,
        None,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert_eq!((charge.g(), charge.p()), (2_684, 0));
    let second = prepare_complete_policy_replace(
        Some(first.value()),
        0,
        Some(1),
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert_eq!((charge.g(), charge.p()), (7_925, 0));
    assert_eq!(first.value().generation(), 1);
    assert_eq!(second.value().generation(), 2);
    assert_eq!(second.expected_generation(), Some(1));
    assert!(first.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    assert!(second.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    let result =
        decide_complete_policy_replace(Some(first.value()), 0, b"r", &second, profile, &mut charge);
    match result {
        Ok(plan) => {
            assert_eq!(maximum, 13_167);
            assert_eq!((charge.g(), charge.p()), (13_167, 0));
            assert_eq!(plan.expected_generation(), 1);
            assert_eq!(plan.value().generation(), 2);
            assert!(plan.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
            assert_eq!(plan.value().encoded(), second.value().encoded());
            assert!(std::ptr::eq(plan.value(), second.value()));
        }
        Err(error) => {
            assert_eq!(maximum, 13_166);
            assert_eq!(
                error,
                Schema(WorkBytes {
                    bytes: 13_167,
                    maximum: 13_166
                })
            );
            assert_eq!((charge.g(), charge.p()), (13_165, 0));
        }
    }
    assert_replacement_refusals(&first, &second, profile, &mut charge, maximum)
}

fn assert_replacement_refusals(
    first: &wf_domain::policy::complete::PreparedCompletePolicyReplace,
    second: &wf_domain::policy::complete::PreparedCompletePolicyReplace,
    profile: wf_domain::policy::complete::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum: u64,
) -> TestResult {
    let before = (charge.g(), charge.p());
    let result =
        decide_complete_policy_replace(Some(first.value()), 0, b"r", second, profile, charge);
    assert!(
        matches!(result, Err(Schema(WorkBytes { bytes, maximum: cap }))
        if bytes == before.0 + 128 && cap == maximum)
    );
    assert_eq!((charge.g(), charge.p()), before);
    let other = limits(maximum + 1)?;
    let result =
        decide_complete_policy_replace(Some(first.value()), 0, b"r", second, other, charge);
    assert!(matches!(result, Err(Schema(ProfileMismatch))));
    assert_eq!((charge.g(), charge.p()), before);
    Ok(())
}

#[test]
fn complete_replace_preserves_standalone_cas_before_current_validation() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let current = prepare_complete_policy_replace(
        None,
        0,
        None,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    let corrupt = bind(
        candidate(
            1,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        )?,
        vec![0; 40],
        0..22,
        profile,
        &mut charge,
    )?;
    let bad_bytes = corrupt.encoded().to_vec();
    for (existing, expected, invalid_new, wanted) in [
        (
            Some(&corrupt),
            Some(0),
            true,
            Schema(Policy(PolicyError::StaleGeneration)),
        ),
        (Some(&corrupt), Some(1), true, EncodingMismatch),
        (
            Some(current.value()),
            Some(1),
            true,
            Schema(Policy(PolicyError::InvalidMaxActive)),
        ),
        (
            Some(current.value()),
            Some(1),
            false,
            Schema(InvalidAuthority),
        ),
    ] {
        let mut input = data(b"r".to_vec(), None, BTreeMap::new());
        if invalid_new {
            input.max_active_ms = Some(0);
        }
        let before = charge.g();
        let result = prepare_complete_policy_replace(
            existing,
            MAX_POLICY_GENERATION + 1,
            expected,
            input,
            profile,
            &mut charge,
        );
        assert!(matches!(result, Err(error) if error == wanted));
        if expected == Some(0) {
            assert_eq!(charge.g(), before + 128);
        }
    }
    assert_decide_replace_priority(&current, &corrupt, profile, &mut charge)?;
    let other = limits(20_000_001)?;
    let before = (charge.g(), charge.p());
    let result = prepare_complete_policy_replace(
        Some(&corrupt),
        MAX_POLICY_GENERATION + 1,
        Some(0),
        data(Vec::new(), None, BTreeMap::new()),
        other,
        &mut charge,
    );
    assert!(matches!(result, Err(Schema(ProfileMismatch))));
    assert_eq!((charge.g(), charge.p()), before);
    assert_eq!(corrupt.generation(), 1);
    assert!(corrupt.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    assert_eq!(corrupt.encoded(), bad_bytes);
    assert_eq!(current.value().generation(), 1);
    assert!(current.value().data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    Ok(())
}
