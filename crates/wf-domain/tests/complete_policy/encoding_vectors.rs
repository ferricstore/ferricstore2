//! Independently specified normalized V2 bytes and presence controls.

use std::collections::BTreeMap;
use wf_domain::policy::complete::LimitData;

use wf_domain::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, CompletePolicyEncodingError,
    CompleteStatePolicyData, DenialAuditMode, EffectsData, GovernanceData, GovernanceMode,
    PolicySchemaCharge, decode_complete_policy_value_v2, prepare_complete_policy_value_v2,
    verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::LimitEnforcement;

use super::{TestResult, candidate, data, limits};

#[test]
fn v2_minimal_literal_bytes_preserve_the_v1_format() -> TestResult {
    // Independent 18-byte FWPV wrapper and 22-byte FWPS snapshot, not codec output.
    let expected =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    assert_eq!(expected.len(), 40);
    let profile = limits(100_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let input = candidate(
        1,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    let value = prepare_complete_policy_value_v2(input, profile, &mut charge)?;
    assert_eq!(value.generation(), 1);
    assert_eq!(value.data().workflow_type, b"r");
    assert_eq!(value.encoded(), expected);
    let decoded = decode_complete_policy_value_v2(expected, profile, &mut charge)?;
    assert!(decoded == value);
    verify_complete_policy_value_v2(&value, profile, &mut charge)?;
    // Version 1 remains the unchanged ports contract; this domain entry does not upcast it.
    let mut version_one = expected.to_vec();
    version_one[5] = 1;
    version_one[23] = 1;
    assert!(matches!(
        decode_complete_policy_value_v2(&version_one, profile, &mut charge),
        Err(CompletePolicyEncodingError::UnsupportedVersion)
    ));
    Ok(())
}

#[test]
fn v2_all_fields_round_trip_without_losing_presence() -> TestResult {
    let expected = [
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\xa5FWPS\0\x02".as_slice(),
        b"\0\0\0\x01r\x01\0\0\0\0\x01v\x01\0\0\0\0\0\0\0\x01",
        b"\x01\x01\0\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\0\0\0\x01q",
        b"\x01\x01\0\0\0\0\0\0\0\x01\x01\0\0\0\x01",
        b"\0\0\0\x01\0\0\0\x01s\x01\x01",
        b"\x01\0\0\0\x01\0\0\x01\x01\0\0\x01\0\x01\x01\0\0\0\0",
        b"\x01\0\0\0\0\x01\0\0\0\x01m\x01\x01\x02",
        b"\x01\x01\0\0\0\0\0\x01\0\x01\x01\0\x01\0",
        b"\x01\0\0\0\0\x01\0\0\0\0\x01\0\0\0\0",
        b"\x01\x01\0\0\0\0\0\x01\0\0\0\0\x01\0\0\0\0\0\0\0\x01",
    ]
    .concat();
    assert_eq!(expected.len(), 183);
    let mut input = data(
        b"r".to_vec(),
        Some(PolicyVersion::Bytes(b"v".to_vec())),
        BTreeMap::new(),
    );
    input.max_active_ms = Some(1);
    input.retry = Some(RetryOverride {
        max_retries: Some(0),
        backoff: Some(Backoff {
            kind: BackoffKind::None,
            base_ms: 0,
            max_ms: 0,
            jitter_pct: 0,
        }),
        exhausted_to: Some(b"q".to_vec()),
    });
    input.retention = Some(RetentionOverride {
        ttl_ms: Some(1),
        history_max_events: Some(1),
    });
    input.states.insert(
        b"s".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Fifo),
            retry: Some(RetryOverride::default()),
            retention: Some(RetentionOverride::default()),
            governance: Some(GovernanceData {
                mode: Some(GovernanceMode::Minimal),
                effects: None,
                audit: Some(AuditData {
                    effect_events: None,
                    denials: Some(DenialAuditMode::Sampled),
                }),
                limits: None,
                budgets: None,
                circuits: None,
                approvals: None,
            }),
        },
    );
    input.indexed_attributes = Some(Vec::new());
    input.indexed_state_meta = Some(b"m".to_vec());
    input.governance = Some(GovernanceData {
        mode: Some(GovernanceMode::Full),
        effects: Some(EffectsData {
            allowed: Some(Vec::new()),
            denied: None,
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
            states: None,
            assignees: Some(Vec::new()),
            timeout_ms: Some(1),
        }),
    });
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let decoded = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
    assert!(decoded.data() == &input);
    assert_eq!(decoded.generation(), 1);
    assert_eq!(decoded.encoded(), expected);
    let admitted = candidate(1, input, profile, &mut charge)?;
    let prepared = prepare_complete_policy_value_v2(admitted, profile, &mut charge)?;
    assert!(prepared == decoded);
    verify_complete_policy_value_v2(&prepared, profile, &mut charge)?;
    Ok(())
}

#[test]
fn v2_lists_retain_order_and_maps_retain_byte_order() -> TestResult {
    let expected = [
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\x01\x2cFWPS\0\x02\0\0\0\x01t\0\0\0\0".as_slice(),
        b"\0\0\0\x02\0\0\0\x01a\x01\0\0\0\0\0\0\0\x01\xff\0\0\0\0",
        b"\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01a\0\x01\x01\x01",
        b"\x01\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01a\x01\0\0\0\0\x01\x01",
        b"\x01\x01\x01\x01\x02\x01\0\0\0\x02\0\0\0\x01a",
        b"\0\0\0\0\0\0\0\0\x01\0\x01\0\0\0\0\0\0\0\x01\0\0\0\x01\xff",
        b"\0\x1f\xff\xff\xff\xff\xff\xff\x01\x01\0",
        b"\x01\0\0\0\x01\0\0\0\x01b\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01",
        b"\0\x01\0\0\0\0\0\0\0\x02\x01\0\0\0\x01\xff",
        b"\x01\0\0\0\x01\0\0\0\x01c\0\0\0\0\0\0\0\x41\0\0\0\0\0\0\0\x01",
        b"\x01\0\0\0\0\0\0\0\x02\x01\0\0\0\0\0\0\0\x03\x01\x64",
        b"\x01\0\0\0\0\0\0\0\x04\x01\0\0\0\0\0\0\0\x05\x01\0\0\0\0\0\0\0\x06",
        b"\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01a",
        b"\x01\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01a\x01\0\0\0\0",
        b"\x01\0\0\0\x01\0\0\0\x01\xff\x01\0\0\0\0\0\0\0\x07",
    ]
    .concat();
    assert_eq!(expected.len(), 318);
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let value = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
    let content = value.data();
    assert_eq!(content.workflow_type, b"t");
    assert_eq!(
        content.states.keys().map(Vec::as_slice).collect::<Vec<_>>(),
        [b"a".as_slice(), b"\xff".as_slice()]
    );
    assert_eq!(
        content.states[b"a".as_slice()].mode,
        Some(StateMode::Parallel)
    );
    assert!(content.states[b"\xff".as_slice()].mode.is_none());
    let ordered = [b"z".to_vec(), b"a".to_vec()];
    assert_eq!(
        content.indexed_attributes.as_deref(),
        Some(ordered.as_slice())
    );
    let governance = content.governance.as_ref().ok_or("missing governance")?;
    assert_eq!(governance.mode, Some(GovernanceMode::Ledger));
    let effects = governance.effects.as_ref().ok_or("missing effects")?;
    assert_eq!(effects.allowed.as_deref(), Some(ordered.as_slice()));
    assert_eq!(effects.denied.as_deref(), Some(<&[Vec<u8>]>::default()));
    assert_eq!(effects.require_idempotency_key, Some(true));
    let audit = governance.audit.ok_or("missing audit")?;
    assert_eq!(
        (audit.effect_events, audit.denials),
        (Some(true), Some(DenialAuditMode::All))
    );
    let rules = governance.limits.as_ref().ok_or("missing limits")?;
    assert_eq!(rules.len(), 2);
    let strict = LimitData {
        limit: 0,
        enforcement: Some(LimitEnforcement::StrictGlobal),
        lease_size: Some(1),
    };
    assert!(rules[b"a".as_slice()] == strict);
    let approximate = LimitData {
        limit: MAX_POLICY_GENERATION,
        enforcement: Some(LimitEnforcement::ApproximateGlobal),
        lease_size: None,
    };
    assert!(rules[b"\xff".as_slice()] == approximate);
    let budget = &governance.budgets.as_ref().ok_or("missing budgets")?[b"b".as_slice()];
    let expected_budget = BudgetData {
        limit: 0,
        window_ms: 1,
        enforcement: None,
        lease_size: Some(2),
        unit: Some(vec![0xff]),
    };
    assert!(budget == &expected_budget);
    let circuit = &governance.circuits.as_ref().ok_or("missing circuits")?[b"c".as_slice()];
    let expected_circuit = CircuitData {
        failure_threshold: 65,
        open_ms: 1,
        window_ms: Some(2),
        min_calls: Some(3),
        failure_rate_pct: Some(100),
        latency_threshold_ms: Some(4),
        half_open_max_probes: Some(5),
        half_open_success_threshold: Some(6),
        error_classes: Some(vec![b"z".to_vec(), b"a".to_vec()]),
    };
    assert!(circuit == &expected_circuit);
    let approvals = governance.approvals.as_ref().ok_or("missing approvals")?;
    let expected_approvals = ApprovalsData {
        effects: Some(vec![b"z".to_vec(), b"a".to_vec()]),
        states: Some(Vec::new()),
        assignees: Some(vec![b"\xff".to_vec()]),
        timeout_ms: Some(7),
    };
    assert!(approvals == &expected_approvals);
    assert_eq!(value.encoded(), expected);
    verify_complete_policy_value_v2(&value, profile, &mut charge)?;
    Ok(())
}

#[test]
fn v2_versions_and_generations_keep_distinct_integer_domains() -> TestResult {
    let profile = limits(1_000_000)?;
    let cases: [(u64, Option<PolicyVersion>, &[u8]); 4] = [
        (1, None, b"\0"),
        (
            1,
            Some(PolicyVersion::Bytes(vec![0xff])),
            b"\x01\0\0\0\0\x01\xff",
        ),
        (
            MAX_POLICY_GENERATION,
            Some(PolicyVersion::Number(0)),
            b"\x01\x01\0\0\0\0\0\0\0\0",
        ),
        (
            1,
            Some(PolicyVersion::Number(u64::MAX)),
            b"\x01\x01\xff\xff\xff\xff\xff\xff\xff\xff",
        ),
    ];
    for (generation, version, version_bytes) in cases {
        let snapshot = [
            b"FWPS\0\x02\0\0\0\x01r".as_slice(),
            version_bytes,
            b"\0\0\0\0\0\0\0\0\0\0",
        ]
        .concat();
        let length = u32::try_from(snapshot.len())?.to_be_bytes();
        let expected = [
            b"FWPV\0\x02".as_slice(),
            &generation.to_be_bytes(),
            &length,
            &snapshot,
        ]
        .concat();
        let mut charge = PolicySchemaCharge::new(profile);
        let input = data(b"r".to_vec(), version, BTreeMap::new());
        let decoded = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
        assert_eq!(decoded.generation(), generation);
        assert!(decoded.data() == &input);
        let admitted = candidate(generation, input, profile, &mut charge)?;
        let prepared = prepare_complete_policy_value_v2(admitted, profile, &mut charge)?;
        assert_eq!(prepared.encoded(), expected);
        assert!(prepared == decoded);
    }
    for generation in [0, MAX_POLICY_GENERATION + 1] {
        let mut encoded =
            b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0"
                .to_vec();
        encoded[6..14].copy_from_slice(&generation.to_be_bytes());
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::Schema(
                wf_domain::policy::complete::PolicySchemaError::Policy(
                    PolicyError::InvalidGeneration
                )
            ))
        ));
    }
    for (kind, tag) in [
        (BackoffKind::Fixed, 1_u8),
        (BackoffKind::Linear, 2),
        (BackoffKind::Exponential, 3),
    ] {
        let expected = [
            b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x2bFWPS\0\x02\0\0\0\x01r\0\0\x01\0\x01".as_slice(),
            &[tag],
            b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        ]
        .concat();
        assert_eq!(expected.len(), 61);
        let mut input = data(b"r".to_vec(), None, BTreeMap::new());
        input.retry = Some(RetryOverride {
            max_retries: None,
            backoff: Some(Backoff {
                kind,
                base_ms: 0,
                max_ms: 0,
                jitter_pct: 0,
            }),
            exhausted_to: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        let decoded = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
        assert!(decoded.data() == &input);
        let admitted = candidate(1, input, profile, &mut charge)?;
        let prepared = prepare_complete_policy_value_v2(admitted, profile, &mut charge)?;
        assert_eq!(prepared.encoded(), expected);
        assert!(prepared == decoded);
    }
    Ok(())
}

#[test]
fn v2_error_class_count_accepts_the_full_governance_list_limit() -> TestResult {
    let profile = limits(20_000_000)?;
    for count in [257_u16, 1000] {
        let names: Vec<Vec<u8>> = (0..count).map(|n| n.to_be_bytes().to_vec()).collect();
        // Independent grammar: 60 bytes before count, then count + names + approvals None.
        let mut snapshot = [
            b"FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\x01".as_slice(),
            b"\0\0\0\0\0\x01\0\0\0\x01\0\0\0\x01k",
            b"\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0\x01",
            b"\0\0\0\0\0\0\x01",
        ]
        .concat();
        assert_eq!(snapshot.len(), 60);
        snapshot.extend_from_slice(&u32::from(count).to_be_bytes());
        for name in &names {
            snapshot.extend_from_slice(&2_u32.to_be_bytes());
            snapshot.extend_from_slice(name);
        }
        snapshot.push(0);
        assert_eq!(snapshot.len(), 65 + 6 * usize::from(count));
        let expected = [
            b"FWPV\0\x02\0\0\0\0\0\0\0\x01".as_slice(),
            &u32::try_from(snapshot.len())?.to_be_bytes(),
            &snapshot,
        ]
        .concat();
        assert_eq!(expected.len(), 83 + 6 * usize::from(count));
        let mut input = data(b"r".to_vec(), None, BTreeMap::new());
        input.governance = Some(GovernanceData {
            mode: None,
            effects: None,
            audit: None,
            limits: None,
            budgets: None,
            circuits: Some(BTreeMap::from([(
                b"k".to_vec(),
                CircuitData {
                    failure_threshold: 1,
                    open_ms: 1,
                    window_ms: None,
                    min_calls: None,
                    failure_rate_pct: None,
                    latency_threshold_ms: None,
                    half_open_max_probes: None,
                    half_open_success_threshold: None,
                    error_classes: Some(names),
                },
            )])),
            approvals: None,
        });
        let mut charge = PolicySchemaCharge::new(profile);
        let decoded = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
        assert!(decoded.data() == &input);
        assert_eq!(decoded.generation(), 1);
        assert_eq!(decoded.encoded(), expected);
        let admitted = candidate(1, input, profile, &mut charge)?;
        let prepared = prepare_complete_policy_value_v2(admitted, profile, &mut charge)?;
        assert_eq!(prepared.encoded(), expected);
        assert!(prepared == decoded);
        verify_complete_policy_value_v2(&prepared, profile, &mut charge)?;
    }
    Ok(())
}

#[test]
fn v2_error_class_bytes_refuse_before_semantic_materialization() -> TestResult {
    let profile = limits(20_000_000)?;
    for length in [256_usize, 257] {
        let name = vec![b'a'; length];
        let mut snapshot = [
            b"FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\x01".as_slice(),
            b"\0\0\0\0\0\x01\0\0\0\x01\0\0\0\x01k",
            b"\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0\x01",
            b"\0\0\0\0\0\0\x01",
        ]
        .concat();
        assert_eq!(snapshot.len(), 60);
        snapshot.extend_from_slice(&1_u32.to_be_bytes());
        snapshot.extend_from_slice(&u32::try_from(length)?.to_be_bytes());
        snapshot.extend_from_slice(&name);
        snapshot.push(0);
        assert_eq!(snapshot.len(), 69 + length);
        let expected = [
            b"FWPV\0\x02\0\0\0\0\0\0\0\x01".as_slice(),
            &u32::try_from(snapshot.len())?.to_be_bytes(),
            &snapshot,
        ]
        .concat();
        assert_eq!(expected.len(), 87 + length);
        let mut charge = PolicySchemaCharge::new(profile);
        if length == 257 {
            assert!(matches!(
                decode_complete_policy_value_v2(&expected, profile, &mut charge),
                Err(CompletePolicyEncodingError::ByteLimit)
            ));
            // Envelope/root/scalars/RAW nodes and length are admitted; payload is not.
            assert_eq!((charge.g(), charge.p()), (2006, 0));
        } else {
            let decoded = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
            let classes = decoded
                .data()
                .governance
                .as_ref()
                .and_then(|governance| governance.circuits.as_ref())
                .and_then(|circuits| circuits.get(b"k".as_slice()))
                .and_then(|circuit| circuit.error_classes.as_ref())
                .ok_or("missing error classes")?;
            assert_eq!(classes.as_slice(), [name].as_slice());
            assert_eq!(decoded.generation(), 1);
            assert_eq!(decoded.encoded(), expected);
            verify_complete_policy_value_v2(&decoded, profile, &mut charge)?;
        }
    }
    Ok(())
}

#[test]
fn source_policy_generation_stops_at_exact_migration_score_boundary() -> TestResult {
    // Pinned PolicyCommandTest:549-564; normalized encoding replaces ETF here.
    assert_eq!(MAX_POLICY_GENERATION, 9_007_199_254_740_991);
    let expected = b"FWPV\0\x02\0\x1f\xff\xff\xff\xff\xff\xff\0\0\0\x36FWPS\0\x02\0\0\0\x13generation-boundary\x01\0\0\0\0\x09public-v1\0\0\0\0\0\0\0\0\0\0";
    assert_eq!(expected.len(), 72);
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let decoded = decode_complete_policy_value_v2(expected, profile, &mut charge)?;
    assert_eq!(decoded.generation(), MAX_POLICY_GENERATION);
    assert!(
        decoded.data()
            == &data(
                b"generation-boundary".to_vec(),
                Some(PolicyVersion::Bytes(b"public-v1".to_vec())),
                BTreeMap::new(),
            )
    );
    let mut invalid = expected.to_vec();
    invalid[6..14].copy_from_slice(&(MAX_POLICY_GENERATION + 1).to_be_bytes());
    assert!(matches!(
        decode_complete_policy_value_v2(&invalid, profile, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            wf_domain::policy::complete::PolicySchemaError::Policy(PolicyError::InvalidGeneration)
        ))
    ));
    let maximum = wf_domain::policy::complete::prepare_complete_policy_replace(
        None,
        MAX_POLICY_GENERATION - 1,
        None,
        data(
            b"generation-boundary".to_vec(),
            Some(PolicyVersion::Bytes(b"public-v1".to_vec())),
            BTreeMap::new(),
        ),
        profile,
        &mut charge,
    )?;
    assert_eq!(maximum.value().generation(), MAX_POLICY_GENERATION);
    assert_eq!(maximum.value().encoded(), expected);
    let refusal = wf_domain::policy::complete::prepare_complete_policy_replace(
        None,
        MAX_POLICY_GENERATION,
        None,
        data(
            b"generation-boundary".to_vec(),
            Some(PolicyVersion::Bytes(b"public-v1".to_vec())),
            BTreeMap::new(),
        ),
        profile,
        &mut charge,
    )
    .err()
    .ok_or("generation at the source ceiling must refuse")?;
    assert_eq!(
        refusal,
        CompletePolicyEncodingError::Schema(
            wf_domain::policy::complete::PolicySchemaError::Policy(
                PolicyError::GenerationExhausted
            )
        )
    );
    assert_eq!(refusal.to_string(), "ERR flow policy generation exhausted");
    assert_eq!(decoded.encoded(), expected);
    assert_eq!(maximum.value().generation(), MAX_POLICY_GENERATION);
    Ok(())
}
