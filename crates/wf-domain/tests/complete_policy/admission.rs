use super::{TestResult, bind, candidate, data, limits};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    BudgetData, CircuitData, CircuitTrackingFailure, CompleteStatePolicyData, EffectsData,
    GovernanceData, GovernanceField, GovernanceRuleKind, LimitData, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, resolve_governance, restore_complete_policy,
    validate_stored_complete_policy,
};
use wf_domain::policy::installation::{PolicyError, PolicyLimits};
use wf_domain::workflow::admission::AdmissionError;

#[test]
fn normalized_schema_covers_source_scalar_and_circuit_extremes() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let exact = 9_007_199_254_740_991;
    for (limit, lease, window, error) in [
        (0, None, 1, None),
        (exact, Some(exact), exact, None),
        (
            exact + 1,
            None,
            1,
            Some(PolicySchemaError::InvalidInteger(GovernanceField::Limit)),
        ),
        (
            0,
            Some(0),
            1,
            Some(PolicySchemaError::InvalidInteger(
                GovernanceField::LeaseSize,
            )),
        ),
        (
            0,
            None,
            0,
            Some(PolicySchemaError::InvalidInteger(
                GovernanceField::WindowMillis,
            )),
        ),
    ] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        value.governance = Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            limits: None,
            budgets: Some(BTreeMap::from([(
                b"cpu".to_vec(),
                BudgetData {
                    limit,
                    window_ms: window,
                    enforcement: None,
                    lease_size: lease,
                    unit: Some(b"ms".to_vec()),
                },
            )])),
            circuits: None,
            approvals: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            error
        );
    }
    check_circuit_extremes(profile);
    for active in [0, 31_536_000_001] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        value.max_active_ms = Some(active);
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            Some(PolicySchemaError::Policy(PolicyError::InvalidMaxActive))
        );
    }
    assert_eq!(
        PolicySchemaLimits::new(PolicyLimits::default(), 0).err(),
        Some(AdmissionError::InvalidLimits)
    );
    assert_ne!(PolicySchemaError::LengthOverflow.to_string(), "");
    // No forged account or infeasible repeated operation is claimed as executed u64 overflow.
    Ok(())
}

#[test]
fn opaque_names_and_canonical_metadata_keep_separate_rules() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let whitespace = [
        0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x20, 0x85, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003,
        0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f,
        0x3000,
    ];
    for metadata in [false, true] {
        for point in whitespace {
            let ch = char::from_u32(point).ok_or("literal scalar")?;
            for key in [
                ch.to_string(),
                format!("{ch}x"),
                format!("x{ch}"),
                format!("x{ch}y"),
            ] {
                check_metadata_name(profile, metadata, &key);
            }
        }
        check_metadata_owners(profile, metadata)?;
    }
    let mut value = data(vec![b'r'], None, BTreeMap::new());
    value.governance = Some(GovernanceData {
        mode: None,
        effects: Some(EffectsData {
            allowed: Some(vec![vec![0xff], b" __ ".to_vec(), vec![0]]),
            denied: None,
            require_idempotency_key: None,
        }),
        audit: None,
        limits: Some(BTreeMap::from([(
            vec![0xff],
            LimitData {
                limit: 0,
                enforcement: None,
                lease_size: None,
            },
        )])),
        budgets: None,
        circuits: None,
        approvals: None,
    });
    let mut charge = PolicySchemaCharge::new(profile);
    let stored = bind(
        candidate(1, value, profile, &mut charge)?,
        vec![1],
        0..1,
        profile,
        &mut charge,
    )?;
    let resolved = resolve_governance(&stored, None, profile, &mut charge)?;
    assert_eq!(
        resolved.effects().allowed(),
        &[vec![0xff], b" __ ".to_vec(), vec![0]]
    );
    assert_eq!(
        resolved.limit(&[0xff], &mut charge)?.ok_or("opaque")?.limit,
        0
    );
    Ok(())
}

#[test]
fn all_count_and_byte_occurrences_precede_duplicate_work() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    for (names, error) in [
        (
            vec![b"same".to_vec(); 1_001],
            PolicySchemaError::ListCount(GovernanceField::Allowed),
        ),
        (
            vec![b"same".to_vec(), b"same".to_vec()],
            PolicySchemaError::NonCanonicalList(GovernanceField::Allowed),
        ),
        (
            vec![b"same".to_vec(), b"same".to_vec(), vec![b'x'; 65_536]],
            PolicySchemaError::ListBytes(GovernanceField::Allowed),
        ),
        (
            vec![Vec::new()],
            PolicySchemaError::InvalidListValue(GovernanceField::Allowed),
        ),
    ] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        value.governance = Some(GovernanceData {
            mode: None,
            effects: Some(EffectsData {
                allowed: Some(names),
                denied: None,
                require_idempotency_key: None,
            }),
            audit: None,
            limits: None,
            budgets: None,
            circuits: None,
            approvals: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            Some(error)
        );
        if matches!(
            error,
            PolicySchemaError::ListCount(_) | PolicySchemaError::ListBytes(_)
        ) {
            assert_eq!(charge.p(), 0);
        }
    }
    check_rule_counts_and_names(profile);
    let states = BTreeMap::from([
        (
            b"s".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: None,
            },
        ),
        (
            b"t".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: None,
                governance: None,
            },
        ),
    ]);
    let small = PolicySchemaLimits::new(PolicyLimits::new(1)?, 128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(small);
    assert_eq!(
        restore_complete_policy(1, data(vec![b'r'], None, states), small, &mut charge).err(),
        Some(PolicySchemaError::Policy(PolicyError::StateBudget))
    );
    assert_eq!(charge.p(), 0);
    Ok(())
}

#[test]
fn shared_schema_work_exact_boundary_and_second_call_do_not_reset() -> TestResult {
    // Literal minimal trace: metrics(128+1+128), conservative core257,
    // then the repeated states-container128. No returned counter defines this oracle.
    for (maximum, accepted) in [(642, true), (641, false)] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(
                1,
                data(vec![b'r'], None, BTreeMap::new()),
                profile,
                &mut charge
            )
            .is_ok(),
            accepted
        );
        assert_eq!(
            (charge.g(), charge.p()),
            if accepted { (642, 0) } else { (514, 0) }
        );
    }
    let profile = limits(1_283)?;
    let mut charge = PolicySchemaCharge::new(profile);
    candidate(
        1,
        data(vec![b'r'], None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    let error = restore_complete_policy(
        1,
        data(vec![b'r'], None, BTreeMap::new()),
        profile,
        &mut charge,
    )
    .err();
    assert_eq!(
        error,
        Some(PolicySchemaError::WorkBytes {
            bytes: 1_284,
            maximum: 1_283
        })
    );
    assert_eq!(charge.g(), 1_156); // second metrics257+core257 committed, refused128 did not.
    for (metadata, maximum) in [(false, 1_998), (true, 1_420)] {
        for admitted in [maximum, maximum - 1] {
            let profile = limits(admitted)?;
            let mut value = data(vec![b'r'], None, BTreeMap::new());
            if metadata {
                value.indexed_state_meta = Some(b"ab".to_vec());
            } else {
                value.indexed_attributes = Some(vec![b"ab".to_vec()]);
            }
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                restore_complete_policy(1, value, profile, &mut charge).is_ok(),
                admitted == maximum
            );
        }
    }
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let stored = bind(
        candidate(
            1,
            data(vec![b'r'], None, BTreeMap::new()),
            profile,
            &mut charge,
        )?,
        vec![1],
        0..1,
        profile,
        &mut charge,
    )?;
    let current = limits(641)?;
    let mut account = PolicySchemaCharge::new(current);
    assert!(matches!(
        validate_stored_complete_policy(&stored, &vec![b'x'; 262_145], current, &mut account),
        Err(PolicySchemaError::WorkBytes {
            bytes: 642,
            maximum: 641
        })
    ));
    let mut mismatch = PolicySchemaCharge::new(current);
    assert_eq!(
        validate_stored_complete_policy(&stored, b"", profile, &mut mismatch).err(),
        Some(PolicySchemaError::ProfileMismatch)
    );
    assert_eq!(mismatch.total(), 0);
    Ok(())
}

fn check_circuit_extremes(profile: PolicySchemaLimits) {
    let exact = 9_007_199_254_740_991;
    for (threshold, min, rate, error) in [
        (64, Some(64), Some(100), None),
        (65, Some(64), Some(1), None),
        (
            65,
            Some(65),
            None,
            Some(PolicySchemaError::CircuitTracking(
                CircuitTrackingFailure::MinCallsAbove64,
            )),
        ),
        (
            65,
            None,
            None,
            Some(PolicySchemaError::CircuitTracking(
                CircuitTrackingFailure::ThresholdAbove64WithoutRate,
            )),
        ),
        (
            65,
            None,
            Some(50),
            Some(PolicySchemaError::CircuitTracking(
                CircuitTrackingFailure::MissingMinCallsForLargeThreshold,
            )),
        ),
        (1, None, Some(0), Some(PolicySchemaError::InvalidPercent)),
        (1, None, Some(101), Some(PolicySchemaError::InvalidPercent)),
        (
            0,
            None,
            None,
            Some(PolicySchemaError::InvalidInteger(
                GovernanceField::FailureThreshold,
            )),
        ),
    ] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        value.governance = Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            limits: None,
            budgets: None,
            circuits: Some(BTreeMap::from([(
                b"api".to_vec(),
                CircuitData {
                    failure_threshold: threshold,
                    open_ms: exact,
                    window_ms: Some(exact),
                    min_calls: min,
                    failure_rate_pct: rate,
                    latency_threshold_ms: Some(exact),
                    half_open_max_probes: Some(exact),
                    half_open_success_threshold: Some(exact),
                    error_classes: None,
                },
            )])),
            approvals: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            error
        );
    }
}

fn check_metadata_owners(profile: PolicySchemaLimits, metadata: bool) -> TestResult {
    for key in [
        vec![0],
        vec![0x1c],
        vec![0x1f],
        "\u{180e}".as_bytes().to_vec(),
        "\u{200b}".as_bytes().to_vec(),
        "\u{feff}".as_bytes().to_vec(),
        "\u{fdd0}".as_bytes().to_vec(),
        b"x__y".to_vec(),
        "e\u{301}".as_bytes().to_vec(),
        vec![b'x'; 64],
        "é".repeat(32).into_bytes(),
    ] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        let owner = key.as_ptr();
        if metadata {
            value.indexed_state_meta = Some(key);
        } else {
            value.indexed_attributes = Some(vec![key]);
        }
        let mut charge = PolicySchemaCharge::new(profile);
        let restored = candidate(1, value, profile, &mut charge)?;
        let actual = if metadata {
            restored.data().indexed_state_meta.as_ref().ok_or("meta")?
        } else {
            &restored
                .data()
                .indexed_attributes
                .as_ref()
                .ok_or("attributes")?[0]
        };
        assert_eq!(actual.as_ptr(), owner);
        let stored = bind(restored, vec![1], 0..1, profile, &mut charge)?;
        validate_stored_complete_policy(&stored, b"r", profile, &mut charge)?;
    }
    for key in [
        Vec::new(),
        vec![0xff],
        vec![0xc2],
        vec![0xed, 0xa0, 0x80],
        b"__x".to_vec(),
        vec![b'x'; 65],
        "é".repeat(33).into_bytes(),
    ] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        if metadata {
            value.indexed_state_meta = Some(key);
        } else {
            value.indexed_attributes = Some(vec![key]);
        }
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            Some(if metadata {
                PolicySchemaError::InvalidIndexedStateMeta
            } else {
                PolicySchemaError::InvalidIndexedAttributes
            })
        );
    }
    Ok(())
}

fn check_rule_counts_and_names(profile: PolicySchemaLimits) {
    for (count, error) in [
        (1_000, None),
        (
            1_001,
            Some(PolicySchemaError::RuleCount(GovernanceRuleKind::Limits)),
        ),
    ] {
        let rules = (0..count)
            .map(|i| {
                (
                    format!("r{i}").into_bytes(),
                    LimitData {
                        limit: 0,
                        enforcement: None,
                        lease_size: None,
                    },
                )
            })
            .collect();
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        value.governance = Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            limits: Some(rules),
            budgets: None,
            circuits: None,
            approvals: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            error
        );
    }
    for (name, error) in [
        (vec![b'x'; 65_535], None),
        (
            vec![b'x'; 65_536],
            Some(PolicySchemaError::NameBytes(GovernanceRuleKind::Limits)),
        ),
        (
            Vec::new(),
            Some(PolicySchemaError::InvalidName(GovernanceRuleKind::Limits)),
        ),
    ] {
        let mut value = data(vec![b'r'], None, BTreeMap::new());
        value.governance = Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            limits: Some(BTreeMap::from([(
                name,
                LimitData {
                    limit: 0,
                    enforcement: None,
                    lease_size: None,
                },
            )])),
            budgets: None,
            circuits: None,
            approvals: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            restore_complete_policy(1, value, profile, &mut charge).err(),
            error
        );
    }
}

fn check_metadata_name(profile: PolicySchemaLimits, metadata: bool, key: &str) {
    let valid = key.starts_with('x') && key.ends_with('y');
    let mut value = data(vec![b'r'], None, BTreeMap::new());
    if metadata {
        value.indexed_state_meta = Some(key.as_bytes().to_vec());
    } else {
        value.indexed_attributes = Some(vec![key.as_bytes().to_vec()]);
    }
    let mut charge = PolicySchemaCharge::new(profile);
    let result = restore_complete_policy(1, value, profile, &mut charge);
    let error = if metadata {
        PolicySchemaError::InvalidIndexedStateMeta
    } else {
        PolicySchemaError::InvalidIndexedAttributes
    };
    assert_eq!(result.err(), (!valid).then_some(error));
}
