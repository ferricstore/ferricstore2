use super::{TestResult, bind, candidate, data, limits};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, CompletePolicyData, CompletePolicyValue,
    CompleteStatePolicyData, DenialAuditMode, EffectsData, GovernanceData, GovernanceMode,
    LimitData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, resolve_governance,
    validate_stored_complete_policy,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::active::LimitEnforcement;

#[test]
fn default_type_state_merges_all_known_fields() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let plain = bind(
        candidate(
            1,
            data(b"job".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        )?,
        vec![1],
        0..1,
        profile,
        &mut charge,
    )?;
    let defaults = resolve_governance(&plain, None, profile, &mut charge)?;
    assert_eq!(defaults.mode(), GovernanceMode::Minimal);
    assert_eq!(defaults.effects().allowed(), <&[Vec<u8>]>::default());
    assert_eq!(defaults.effects().denied(), <&[Vec<u8>]>::default());
    assert!(defaults.effects().require_idempotency_key());
    assert!(defaults.audit().effect_events());
    assert_eq!(defaults.audit().denials(), DenialAuditMode::Sampled);
    assert_eq!(defaults.approvals().effects(), <&[Vec<u8>]>::default());
    assert_eq!(defaults.approvals().states(), <&[Vec<u8>]>::default());
    assert_eq!(defaults.approvals().assignees(), <&[Vec<u8>]>::default());
    assert_eq!(defaults.approvals().timeout_ms(), None);
    assert!(defaults.policy_version().is_none());
    let mut value = data(
        b"job".to_vec(),
        Some(PolicyVersion::Bytes(b"v1".to_vec())),
        BTreeMap::new(),
    );
    value.governance = Some(GovernanceData {
        mode: Some(GovernanceMode::Ledger),
        effects: Some(EffectsData {
            allowed: Some(vec![b"a".to_vec(), b"b".to_vec()]),
            denied: Some(vec![b"x".to_vec()]),
            require_idempotency_key: Some(false),
        }),
        audit: Some(AuditData {
            effect_events: Some(false),
            denials: Some(DenialAuditMode::All),
        }),
        limits: None,
        budgets: None,
        circuits: None,
        approvals: Some(ApprovalsData {
            effects: Some(vec![b"pay".to_vec()]),
            states: Some(vec![b"ok".to_vec()]),
            assignees: Some(vec![b"owner".to_vec()]),
            timeout_ms: Some(7),
        }),
    });
    value.states.insert(
        b"ready".to_vec(),
        CompleteStatePolicyData {
            mode: None,
            retry: None,
            retention: None,
            governance: Some(GovernanceData {
                mode: Some(GovernanceMode::Full),
                effects: Some(EffectsData {
                    allowed: Some(Vec::new()),
                    denied: None,
                    require_idempotency_key: None,
                }),
                audit: Some(AuditData {
                    effect_events: Some(true),
                    denials: None,
                }),
                limits: None,
                budgets: None,
                circuits: None,
                approvals: Some(ApprovalsData {
                    effects: None,
                    states: Some(Vec::new()),
                    assignees: None,
                    timeout_ms: None,
                }),
            }),
        },
    );
    let stored = bind(
        candidate(2, value, profile, &mut charge)?,
        vec![2],
        0..1,
        profile,
        &mut charge,
    )?;
    assert_type_state_merge(&stored, profile, &mut charge)?;
    Ok(())
}

#[test]
fn rule_maps_inherit_per_field_and_empty_maps_do_not_erase() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut value = data(b"job".to_vec(), None, BTreeMap::new());
    value.governance = Some(GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: Some(BTreeMap::from([(
            b"running".to_vec(),
            LimitData {
                limit: 4,
                enforcement: Some(LimitEnforcement::StrictGlobal),
                lease_size: Some(8),
            },
        )])),
        budgets: Some(BTreeMap::from([(
            b"cpu".to_vec(),
            BudgetData {
                limit: 10,
                window_ms: 20,
                enforcement: Some(LimitEnforcement::ApproximateGlobal),
                lease_size: Some(3),
                unit: Some(b"ms".to_vec()),
            },
        )])),
        circuits: Some(BTreeMap::from([(
            b"api".to_vec(),
            CircuitData {
                failure_threshold: 2,
                open_ms: 3,
                window_ms: Some(4),
                min_calls: Some(5),
                failure_rate_pct: Some(6),
                latency_threshold_ms: Some(7),
                half_open_max_probes: Some(8),
                half_open_success_threshold: Some(9),
                error_classes: Some(vec![b"timeout".to_vec()]),
            },
        )])),
        approvals: None,
    });
    add_state_rule_overrides(&mut value);
    let mut charge = PolicySchemaCharge::new(profile);
    let stored = bind(
        candidate(3, value, profile, &mut charge)?,
        vec![3],
        0..1,
        profile,
        &mut charge,
    )?;
    assert_complete_rule_merge(&stored, profile, &mut charge)?;
    Ok(())
}

#[test]
fn running_resolution_comes_only_from_complete_bound_content() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    for version in [
        None,
        Some(PolicyVersion::Number(0)),
        Some(PolicyVersion::Number(9_007_199_254_740_991)),
        Some(PolicyVersion::Number(u64::MAX)),
        Some(PolicyVersion::Bytes(vec![0xff])),
    ] {
        let mut value = data(b"job".to_vec(), version.clone(), BTreeMap::new());
        value.governance = Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            limits: Some(BTreeMap::from([(
                b"running".to_vec(),
                LimitData {
                    limit: 3,
                    enforcement: Some(LimitEnforcement::StrictGlobal),
                    lease_size: Some(2),
                },
            )])),
            budgets: None,
            circuits: None,
            approvals: None,
        });
        value.states.insert(
            b"ready".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: Some(GovernanceData {
                    mode: None,
                    effects: None,
                    audit: None,
                    limits: Some(BTreeMap::from([(
                        b"running".to_vec(),
                        LimitData {
                            limit: 0,
                            enforcement: None,
                            lease_size: None,
                        },
                    )])),
                    budgets: None,
                    circuits: None,
                    approvals: None,
                }),
            },
        );
        let mut charge = PolicySchemaCharge::new(profile);
        let stored = bind(
            candidate(7, value, profile, &mut charge)?,
            vec![7],
            0..1,
            profile,
            &mut charge,
        )?;
        validate_stored_complete_policy(&stored, b"job", profile, &mut charge)?;
        let resolved = resolve_governance(&stored, Some(b"ready"), profile, &mut charge)?;
        let rule = resolved.limit(b"running", &mut charge)?.ok_or("running")?;
        assert!(
            (rule.limit, rule.enforcement, rule.lease_size)
                == (0, Some(LimitEnforcement::StrictGlobal), Some(2))
        );
        assert_eq!(stored.generation(), 7);
        if matches!(version, Some(PolicyVersion::Number(u64::MAX))) {
            assert!(resolved.policy_version().is_none());
        } else {
            assert_eq!(resolved.policy_version(), stored.data().version.as_ref());
        }
        assert_eq!(
            validate_stored_complete_policy(&stored, b"foreign", profile, &mut charge).err(),
            Some(PolicySchemaError::InvalidAuthority)
        );
        let absent = resolve_governance(&stored, Some(b"other"), profile, &mut charge)?
            .limit(b"running", &mut charge)?
            .ok_or("fallback")?;
        assert_eq!(absent.limit, 3);
    }
    Ok(())
}

#[test]
fn current_profile_readmission_precedes_name_lookup() -> TestResult {
    let generous = limits(128 * 1_024 * 1_024)?;
    let states = BTreeMap::from([
        (
            b"a".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: None,
            },
        ),
        (
            b"b".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: None,
            },
        ),
    ]);
    let mut charge = PolicySchemaCharge::new(generous);
    let stored = bind(
        candidate(1, data(vec![b'r'], None, states), generous, &mut charge)?,
        vec![1],
        0..1,
        generous,
        &mut charge,
    )?;
    let restrictive = PolicySchemaLimits::new(PolicyLimits::new(1)?, 128 * 1_024 * 1_024)?;
    for state in [None, Some(b"".as_slice()), Some(b"absent".as_slice())] {
        let mut current = PolicySchemaCharge::new(restrictive);
        assert_eq!(
            resolve_governance(&stored, state, restrictive, &mut current).err(),
            Some(PolicySchemaError::InvalidAuthority)
        );
    }
    let mut mismatch = PolicySchemaCharge::new(restrictive);
    assert_eq!(
        resolve_governance(&stored, Some(&vec![b'x'; 262_145]), generous, &mut mismatch).err(),
        Some(PolicySchemaError::ProfileMismatch)
    );
    assert_eq!(mismatch.total(), 0);
    let mut current = PolicySchemaCharge::new(generous);
    assert_eq!(
        validate_stored_complete_policy(&stored, b"", generous, &mut current).err(),
        Some(PolicySchemaError::InvalidAuthority)
    );
    // Current-only sizing precedes empty expected identity, without its query debit.
    let mut admission = PolicySchemaCharge::new(generous);
    resolve_governance(&stored, None, generous, &mut admission)?;
    assert_eq!(current.total(), admission.total());
    for query in [vec![b'x'; 262_144], vec![b'x'; 262_145]] {
        let mut current = PolicySchemaCharge::new(generous);
        let result = resolve_governance(&stored, Some(&query), generous, &mut current);
        assert_eq!(
            result.err(),
            (query.len() > 262_144).then_some(PolicySchemaError::InvalidAuthority)
        );
    }
    let resolved = resolve_governance(&stored, None, generous, &mut charge)?;
    for query in [Vec::new(), vec![b'x'; 65_535], vec![b'x'; 65_536]] {
        let mut current = PolicySchemaCharge::new(generous);
        assert_eq!(
            resolved.limit(&query, &mut current).err(),
            (query.len() > 65_535).then_some(PolicySchemaError::InvalidAuthority)
        );
        assert_eq!(current.total(), 0); // Both maps absent, hence no actual get.
    }
    let mut mismatch = PolicySchemaCharge::new(restrictive);
    assert_eq!(
        resolved.limit(b"running", &mut mismatch).err(),
        Some(PolicySchemaError::ProfileMismatch)
    );
    assert_eq!(mismatch.total(), 0);
    Ok(())
}

fn assert_type_state_merge(
    stored: &CompletePolicyValue,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let resolved = resolve_governance(stored, Some(b"ready"), profile, charge)?;
    assert_eq!(resolved.mode(), GovernanceMode::Full);
    assert_eq!(resolved.effects().allowed(), <&[Vec<u8>]>::default());
    assert_eq!(resolved.effects().denied(), &[b"x".to_vec()]);
    assert!(!resolved.effects().require_idempotency_key());
    assert!(resolved.audit().effect_events());
    assert_eq!(resolved.audit().denials(), DenialAuditMode::All);
    assert_eq!(resolved.approvals().effects(), &[b"pay".to_vec()]);
    assert_eq!(resolved.approvals().states(), <&[Vec<u8>]>::default());
    assert_eq!(resolved.approvals().assignees(), &[b"owner".to_vec()]);
    assert_eq!(resolved.approvals().timeout_ms(), Some(7));
    assert_eq!(resolved.policy_version(), stored.data().version.as_ref());
    let borrowed = {
        let short = resolve_governance(stored, Some(b"ready"), profile, charge)?;
        short.approvals().assignees()
    };
    assert_eq!(borrowed[0], b"owner");
    assert_eq!(
        borrowed[0].as_ptr(),
        stored
            .data()
            .governance
            .as_ref()
            .ok_or("gov")?
            .approvals
            .as_ref()
            .ok_or("approvals")?
            .assignees
            .as_ref()
            .ok_or("assignees")?[0]
            .as_ptr()
    );
    for state in [None, Some(b"absent".as_slice()), Some(b"".as_slice())] {
        let fallback = resolve_governance(stored, state, profile, charge)?;
        assert_eq!(fallback.mode(), GovernanceMode::Ledger);
        assert_eq!(
            fallback.effects().allowed(),
            &[b"a".to_vec(), b"b".to_vec()]
        );
    }
    Ok(())
}

fn add_state_rule_overrides(value: &mut CompletePolicyData) {
    value.states.insert(
        b"ready".to_vec(),
        CompleteStatePolicyData {
            mode: None,
            retry: None,
            retention: None,
            governance: Some(GovernanceData {
                mode: None,
                effects: None,
                audit: None,
                limits: Some(BTreeMap::new()),
                budgets: Some(BTreeMap::from([(
                    b"cpu".to_vec(),
                    BudgetData {
                        limit: 0,
                        window_ms: 1,
                        enforcement: None,
                        lease_size: None,
                        unit: None,
                    },
                )])),
                circuits: Some(BTreeMap::from([(
                    b"api".to_vec(),
                    CircuitData {
                        failure_threshold: 10,
                        open_ms: 11,
                        window_ms: None,
                        min_calls: None,
                        failure_rate_pct: None,
                        latency_threshold_ms: None,
                        half_open_max_probes: None,
                        half_open_success_threshold: None,
                        error_classes: Some(Vec::new()),
                    },
                )])),
                approvals: None,
            }),
        },
    );
}

fn assert_complete_rule_merge(
    stored: &CompletePolicyValue,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let resolved = resolve_governance(stored, Some(b"ready"), profile, charge)?;
    let running = resolved.limit(b"running", charge)?.ok_or("running")?;
    assert!(
        (running.limit, running.enforcement, running.lease_size)
            == (4, Some(LimitEnforcement::StrictGlobal), Some(8))
    );
    let budget = resolved.budget(b"cpu", charge)?.ok_or("budget")?;
    assert!(
        (
            budget.limit(),
            budget.window_ms(),
            budget.enforcement(),
            budget.lease_size(),
            budget.unit()
        ) == (
            0,
            1,
            Some(LimitEnforcement::ApproximateGlobal),
            Some(3),
            Some(b"ms".as_slice())
        )
    );
    let circuit = resolved.circuit(b"api", charge)?.ok_or("circuit")?;
    assert_eq!(
        (
            circuit.failure_threshold(),
            circuit.open_ms(),
            circuit.window_ms(),
            circuit.min_calls(),
            circuit.failure_rate_pct()
        ),
        (10, 11, Some(4), Some(5), Some(6))
    );
    assert_eq!(
        (
            circuit.latency_threshold_ms(),
            circuit.half_open_max_probes(),
            circuit.half_open_success_threshold()
        ),
        (Some(7), Some(8), Some(9))
    );
    assert_eq!(
        circuit.error_classes().ok_or("classes")?,
        <&[Vec<u8>]>::default()
    );
    let original_unit = stored
        .data()
        .governance
        .as_ref()
        .ok_or("gov")?
        .budgets
        .as_ref()
        .ok_or("budgets")?[b"cpu".as_slice()]
    .unit
    .as_ref()
    .ok_or("unit")?;
    assert_eq!(
        budget.unit().ok_or("resolved unit")?.as_ptr(),
        original_unit.as_ptr()
    );
    let borrowed = {
        let short = resolve_governance(stored, None, profile, charge)?;
        short
            .circuit(b"api", charge)?
            .ok_or("circuit")?
            .error_classes()
            .ok_or("classes")?
    };
    assert_eq!(borrowed, &[b"timeout".to_vec()]);
    assert_eq!(
        borrowed[0].as_ptr(),
        stored
            .data()
            .governance
            .as_ref()
            .ok_or("gov")?
            .circuits
            .as_ref()
            .ok_or("circuits")?[b"api".as_slice()]
        .error_classes
        .as_ref()
        .ok_or("classes")?[0]
            .as_ptr()
    );
    assert!(resolved.limit(b"absent", charge)?.is_none());
    assert!(resolved.budget(b"", charge)?.is_none());
    assert!(resolved.circuit(b"absent", charge)?.is_none());
    Ok(())
}
