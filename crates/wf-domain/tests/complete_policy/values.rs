use super::{TestResult, bind, candidate, data, limits};
use std::collections::BTreeMap;
use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, CompletePolicyCandidate, CompletePolicyData,
    CompleteStatePolicyData, DenialAuditMode, EffectsData, GovernanceData, GovernanceMode,
    LimitData, PolicySchemaCharge, PolicySchemaError, restore_complete_policy,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::LimitEnforcement;

#[test]
fn complete_snapshot_moves_every_core_and_governance_owner() -> TestResult {
    let mut value = data(
        b"job".to_vec(),
        Some(PolicyVersion::Bytes(b"v".to_vec())),
        BTreeMap::new(),
    );
    value.max_active_ms = Some(31_536_000_000);
    value.retry = Some(RetryOverride {
        max_retries: Some(1_000),
        backoff: None,
        exhausted_to: Some(b"done".to_vec()),
    });
    value.retention = Some(RetentionOverride {
        ttl_ms: Some(1),
        history_max_events: Some(1_000_000),
    });
    value.indexed_attributes = Some(vec![b"priority".to_vec()]);
    value.indexed_state_meta = Some(b"tag".to_vec());
    value.governance = Some(GovernanceData {
        mode: Some(GovernanceMode::Full),
        effects: Some(EffectsData {
            allowed: Some(vec![b"mail".to_vec()]),
            denied: Some(vec![b"delete".to_vec()]),
            require_idempotency_key: Some(false),
        }),
        audit: Some(AuditData {
            effect_events: Some(false),
            denials: Some(DenialAuditMode::All),
        }),
        limits: Some(BTreeMap::from([(
            b"running".to_vec(),
            LimitData {
                limit: 0,
                enforcement: Some(LimitEnforcement::StrictGlobal),
                lease_size: Some(7),
            },
        )])),
        budgets: Some(BTreeMap::from([(
            b"cpu".to_vec(),
            BudgetData {
                limit: 9,
                window_ms: 10,
                enforcement: None,
                lease_size: Some(3),
                unit: Some(b"ms".to_vec()),
            },
        )])),
        circuits: Some(BTreeMap::from([(
            b"remote".to_vec(),
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
        approvals: Some(ApprovalsData {
            effects: Some(vec![b"pay".to_vec()]),
            states: Some(vec![b"ready".to_vec()]),
            assignees: Some(vec![b"ops".to_vec()]),
            timeout_ms: Some(11),
        }),
    });
    let owners = complete_owner_pointers(&value)?;
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let prepared = candidate(MAX_POLICY_GENERATION, value, profile, &mut charge)?;
    let encoded = vec![0x5a; 17];
    let encoded_owner = encoded.as_ptr();
    let stored = bind(prepared, encoded, 2..15, profile, &mut charge)?;
    assert_eq!(stored.generation(), MAX_POLICY_GENERATION);
    assert_eq!(stored.encoded(), &[0x5a; 17]);
    let value = stored.data();
    assert_complete_owner_values(value)?;
    assert_eq!(stored.encoded().as_ptr(), encoded_owner);
    let after = complete_owner_pointers(value)?;
    assert_eq!(owners, after);
    Ok(())
}

#[test]
fn complete_omission_empty_zero_and_customer_version_are_distinct() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    for version in [
        None,
        Some(PolicyVersion::Number(0)),
        Some(PolicyVersion::Number(u64::MAX)),
        Some(PolicyVersion::Bytes(vec![0xff])),
    ] {
        for present in [false, true] {
            let mut value = data(b"job".to_vec(), version.clone(), BTreeMap::new());
            if present {
                value.retry = Some(RetryOverride::default());
                value.retention = Some(RetentionOverride::default());
                value.indexed_attributes = Some(Vec::new());
                value.governance = Some(GovernanceData {
                    mode: None,
                    effects: Some(EffectsData {
                        allowed: Some(Vec::new()),
                        denied: None,
                        require_idempotency_key: Some(false),
                    }),
                    audit: Some(AuditData {
                        effect_events: Some(false),
                        denials: None,
                    }),
                    limits: Some(BTreeMap::new()),
                    budgets: None,
                    circuits: None,
                    approvals: None,
                });
            }
            let mut charge = PolicySchemaCharge::new(profile);
            let restored = candidate(1, value, profile, &mut charge)?;
            assert_eq!(restored.data().version, version);
            assert_eq!(restored.data().retry.is_some(), present);
            assert_eq!(restored.data().retention.is_some(), present);
            assert_eq!(restored.data().indexed_attributes.is_some(), present);
            assert_eq!(restored.data().governance.is_some(), present);
            if let Some(gov) = &restored.data().governance {
                assert!(gov.limits.as_ref().is_some_and(BTreeMap::is_empty));
                assert!(gov.budgets.is_none());
                assert_eq!(
                    gov.effects
                        .as_ref()
                        .ok_or("effects")?
                        .require_idempotency_key,
                    Some(false)
                );
            }
        }
    }
    for generation in [0, MAX_POLICY_GENERATION + 1] {
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(
                generation,
                data(vec![b'r'], None, BTreeMap::new()),
                profile,
                &mut charge
            )
            .err(),
            Some(PolicySchemaError::Policy(PolicyError::InvalidGeneration))
        );
        assert_eq!(charge.total(), 0);
    }
    {
        let version = Some(PolicyVersion::Bytes(Vec::new()));
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(
                1,
                data(vec![b'r'], version, BTreeMap::new()),
                profile,
                &mut charge
            )
            .err(),
            Some(PolicySchemaError::Policy(PolicyError::InvalidVersion))
        );
    }
    Ok(())
}

#[test]
fn complete_state_maps_keep_presence_and_state_governance_bound() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let state_name = vec![0xff, b's'];
    let state_owner = state_name.as_ptr();
    let target = b"next".to_vec();
    let target_owner = target.as_ptr();
    let states = BTreeMap::from([
        (
            state_name,
            CompleteStatePolicyData {
                mode: Some(StateMode::Fifo),
                retry: Some(RetryOverride {
                    max_retries: Some(0),
                    backoff: None,
                    exhausted_to: Some(target),
                }),
                retention: Some(RetentionOverride {
                    ttl_ms: Some(2),
                    history_max_events: Some(3),
                }),
                governance: Some(GovernanceData {
                    mode: Some(GovernanceMode::Ledger),
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
        ),
        (
            b"other".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: Some(RetryOverride::default()),
                retention: None,
                governance: Some(GovernanceData {
                    mode: None,
                    effects: None,
                    audit: None,
                    limits: Some(BTreeMap::new()),
                    budgets: None,
                    circuits: None,
                    approvals: None,
                }),
            },
        ),
    ]);
    let mut charge = PolicySchemaCharge::new(profile);
    let restored = candidate(1, data(b"job".to_vec(), None, states), profile, &mut charge)?;
    assert_complete_state(&restored, state_owner, target_owner)?;
    for name in [Vec::new(), b"running".to_vec()] {
        let states = BTreeMap::from([(
            name,
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: None,
            },
        )]);
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            restore_complete_policy(1, data(vec![b'r'], None, states), profile, &mut charge),
            Err(PolicySchemaError::Policy(PolicyError::Domain(_)))
        ));
    }
    Ok(())
}

#[test]
fn actual_snapshot_envelope_ranges_have_independent_boundaries() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    for (range, length, error) in [
        (0..262_144, 262_144, None),
        (1..262_145, 263_168, None),
        (0..262_145, 262_145, Some(PolicySchemaError::SnapshotBytes)),
        (0..0, 263_169, Some(PolicySchemaError::EnvelopeBytes)),
        (
            std::ops::Range { start: 3, end: 2 },
            3,
            Some(PolicySchemaError::InvalidRange),
        ),
        (0..4, 3, Some(PolicySchemaError::InvalidRange)),
        (4..4, 3, Some(PolicySchemaError::InvalidRange)),
    ] {
        let mut charge = PolicySchemaCharge::new(profile);
        let prepared = candidate(
            1,
            data(vec![b'r'], None, BTreeMap::new()),
            profile,
            &mut charge,
        )?;
        let encoded = vec![0x33; length];
        let owner = encoded.as_ptr();
        let result = prepared.bind_encoded_snapshot(range, encoded, profile, &mut charge);
        if let Some(error) = error {
            assert_eq!(result.err(), Some(error));
        } else {
            let value = result?;
            assert_eq!(value.encoded().len(), length);
            assert_eq!(value.encoded().as_ptr(), owner);
            assert_eq!(value.data().workflow_type, b"r");
        }
    }
    let mut value = data(vec![b'r'; 262_144], None, BTreeMap::new());
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(restore_complete_policy(1, value, profile, &mut charge).is_ok());
    value = data(vec![b'r'; 262_145], None, BTreeMap::new());
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        restore_complete_policy(1, value, profile, &mut charge).err(),
        Some(PolicySchemaError::SnapshotBytes)
    );
    Ok(())
}

fn complete_owner_pointers(value: &CompletePolicyData) -> TestResult<[*const u8; 22]> {
    let gov = value.governance.as_ref().ok_or("governance")?;
    let effect = gov.effects.as_ref().ok_or("effects")?;
    let budget = gov
        .budgets
        .as_ref()
        .ok_or("budgets")?
        .first_key_value()
        .ok_or("budget")?;
    let circuit = gov
        .circuits
        .as_ref()
        .ok_or("circuits")?
        .first_key_value()
        .ok_or("circuit")?;
    let approval = gov.approvals.as_ref().ok_or("approvals")?;
    let version = match value.version.as_ref().ok_or("version")? {
        PolicyVersion::Bytes(bytes) => bytes,
        PolicyVersion::Number(_) => return Err("byte version".into()),
    };
    let limit = gov
        .limits
        .as_ref()
        .ok_or("limits")?
        .first_key_value()
        .ok_or("limit")?;
    let attributes = value.indexed_attributes.as_ref().ok_or("attributes")?;
    let allowed = effect.allowed.as_ref().ok_or("allowed")?;
    let denied = effect.denied.as_ref().ok_or("denied")?;
    let classes = circuit.1.error_classes.as_ref().ok_or("classes")?;
    let effects = approval.effects.as_ref().ok_or("approval effects")?;
    let states = approval.states.as_ref().ok_or("approval states")?;
    let assignees = approval.assignees.as_ref().ok_or("assignees")?;
    Ok([
        value.workflow_type.as_ptr(),
        value
            .retry
            .as_ref()
            .ok_or("retry")?
            .exhausted_to
            .as_ref()
            .ok_or("target")?
            .as_ptr(),
        value.indexed_attributes.as_ref().ok_or("attributes")?[0].as_ptr(),
        value.indexed_state_meta.as_ref().ok_or("meta")?.as_ptr(),
        effect.allowed.as_ref().ok_or("allowed")?[0].as_ptr(),
        effect.denied.as_ref().ok_or("denied")?[0].as_ptr(),
        budget.0.as_ptr(),
        budget.1.unit.as_ref().ok_or("unit")?.as_ptr(),
        circuit.0.as_ptr(),
        circuit.1.error_classes.as_ref().ok_or("classes")?[0].as_ptr(),
        approval.effects.as_ref().ok_or("approval effects")?[0].as_ptr(),
        approval.states.as_ref().ok_or("approval states")?[0].as_ptr(),
        approval.assignees.as_ref().ok_or("assignees")?[0].as_ptr(),
        version.as_ptr(),
        limit.0.as_ptr(),
        attributes.as_ptr().cast::<u8>(),
        allowed.as_ptr().cast::<u8>(),
        denied.as_ptr().cast::<u8>(),
        classes.as_ptr().cast::<u8>(),
        effects.as_ptr().cast::<u8>(),
        states.as_ptr().cast::<u8>(),
        assignees.as_ptr().cast::<u8>(),
    ])
}

fn assert_complete_owner_values(value: &CompletePolicyData) -> TestResult {
    assert_eq!(value.workflow_type, b"job");
    assert_eq!(value.version, Some(PolicyVersion::Bytes(b"v".to_vec())));
    assert_eq!(value.max_active_ms, Some(31_536_000_000));
    assert_eq!(
        value.retry.as_ref().ok_or("retry")?.max_retries,
        Some(1_000)
    );
    assert_eq!(
        value.retention.ok_or("retention")?.history_max_events,
        Some(1_000_000)
    );
    let retry = value.retry.as_ref().ok_or("retry")?;
    assert!(retry.backoff.is_none());
    assert_eq!(retry.exhausted_to.as_deref(), Some(b"done".as_slice()));
    assert_eq!(value.retention.ok_or("retention")?.ttl_ms, Some(1));
    assert!(value.states.is_empty());
    assert_eq!(value.indexed_attributes, Some(vec![b"priority".to_vec()]));
    assert_eq!(value.indexed_state_meta, Some(b"tag".to_vec()));
    assert_complete_governance(value.governance.as_ref().ok_or("governance")?)
}

fn assert_complete_governance(gov: &GovernanceData) -> TestResult {
    let effect = gov.effects.as_ref().ok_or("effects")?;
    let budget = gov
        .budgets
        .as_ref()
        .ok_or("budgets")?
        .first_key_value()
        .ok_or("budget")?;
    let circuit = gov
        .circuits
        .as_ref()
        .ok_or("circuits")?
        .first_key_value()
        .ok_or("circuit")?;
    let approval = gov.approvals.as_ref().ok_or("approvals")?;
    assert_eq!(gov.mode, Some(GovernanceMode::Full));
    assert_eq!(effect.allowed.as_ref().ok_or("allowed")?[0], b"mail");
    assert_eq!(effect.denied.as_ref().ok_or("denied")?[0], b"delete");
    assert_eq!(effect.require_idempotency_key, Some(false));
    assert_eq!(
        gov.audit.ok_or("audit")?.denials,
        Some(DenialAuditMode::All)
    );
    assert_eq!(
        (budget.1.limit, budget.1.window_ms, budget.1.lease_size),
        (9, 10, Some(3))
    );
    assert_eq!(
        circuit.1.error_classes.as_ref().ok_or("classes")?[0],
        b"timeout"
    );
    assert_eq!(approval.timeout_ms, Some(11));
    assert_eq!(effect.allowed.as_ref().ok_or("allowed")?.len(), 1);
    assert_eq!(effect.denied.as_ref().ok_or("denied")?.len(), 1);
    assert_eq!(gov.audit.ok_or("audit")?.effect_events, Some(false));
    let rules = gov.limits.as_ref().ok_or("limits")?;
    assert_eq!(rules.len(), 1);
    let (name, running) = rules.first_key_value().ok_or("limit")?;
    assert_eq!(name, b"running");
    assert!(
        (running.limit, running.enforcement, running.lease_size)
            == (0, Some(LimitEnforcement::StrictGlobal), Some(7))
    );
    assert_eq!(gov.budgets.as_ref().ok_or("budgets")?.len(), 1);
    assert_eq!(budget.0, b"cpu");
    assert!(budget.1.enforcement.is_none());
    assert_eq!(budget.1.unit.as_deref(), Some(b"ms".as_slice()));
    assert_eq!(gov.circuits.as_ref().ok_or("circuits")?.len(), 1);
    assert_eq!(circuit.0, b"remote");
    assert!(
        circuit.1
            == &CircuitData {
                failure_threshold: 2,
                open_ms: 3,
                window_ms: Some(4),
                min_calls: Some(5),
                failure_rate_pct: Some(6),
                latency_threshold_ms: Some(7),
                half_open_max_probes: Some(8),
                half_open_success_threshold: Some(9),
                error_classes: Some(vec![b"timeout".to_vec()]),
            }
    );
    assert_eq!(approval.effects, Some(vec![b"pay".to_vec()]));
    assert_eq!(approval.states, Some(vec![b"ready".to_vec()]));
    assert_eq!(approval.assignees, Some(vec![b"ops".to_vec()]));
    Ok(())
}

fn assert_complete_state(
    restored: &CompletePolicyCandidate,
    state_owner: *const u8,
    target_owner: *const u8,
) -> TestResult {
    assert_eq!(restored.data().states.len(), 2);
    let (name, state) = restored.data().states.last_key_value().ok_or("state")?;
    assert_eq!(name, &[0xff, b's']);
    assert_eq!(state.mode, Some(StateMode::Fifo));
    assert_eq!(state.retry.as_ref().ok_or("retry")?.max_retries, Some(0));
    assert_eq!(state.retention.ok_or("retention")?.ttl_ms, Some(2));
    assert_eq!(
        state.governance.as_ref().ok_or("governance")?.mode,
        Some(GovernanceMode::Ledger)
    );
    assert!(
        restored.data().states[b"other".as_slice()]
            .governance
            .as_ref()
            .ok_or("other")?
            .limits
            .as_ref()
            .is_some_and(BTreeMap::is_empty)
    );
    let retry = state.retry.as_ref().ok_or("retry")?;
    assert!(retry.backoff.is_none());
    assert_eq!(retry.exhausted_to.as_deref(), Some(b"next".as_slice()));
    assert_eq!(
        state.retention.ok_or("retention")?.history_max_events,
        Some(3)
    );
    let gov = state.governance.as_ref().ok_or("governance")?;
    assert!(gov.effects.is_none());
    assert!(gov.audit.is_none());
    assert!(gov.budgets.is_none());
    assert!(gov.circuits.is_none());
    assert!(gov.approvals.is_none());
    let rules = gov.limits.as_ref().ok_or("limits")?;
    assert_eq!(rules.len(), 1);
    assert!(
        rules[b"running".as_slice()]
            == LimitData {
                limit: 0,
                enforcement: None,
                lease_size: None,
            }
    );
    assert_other_state(restored.data())?;
    assert_eq!(name.as_ptr(), state_owner);
    assert_eq!(
        state
            .retry
            .as_ref()
            .ok_or("retry")?
            .exhausted_to
            .as_ref()
            .ok_or("target")?
            .as_ptr(),
        target_owner
    );
    Ok(())
}

fn assert_other_state(data: &CompletePolicyData) -> TestResult {
    let other = &data.states[b"other".as_slice()];
    assert_eq!(other.mode, None);
    let retry = other.retry.as_ref().ok_or("other retry")?;
    assert_eq!(retry.max_retries, None);
    assert!(retry.backoff.is_none());
    assert!(retry.exhausted_to.is_none());
    assert_eq!(other.retention, None);
    let gov = other.governance.as_ref().ok_or("other governance")?;
    assert_eq!(gov.mode, None);
    assert!(gov.effects.is_none());
    assert!(gov.audit.is_none());
    assert!(gov.budgets.is_none());
    assert!(gov.circuits.is_none());
    assert!(gov.approvals.is_none());
    Ok(())
}
