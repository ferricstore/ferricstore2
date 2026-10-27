//! Scalar variants and independent local field presence for Complete Replace intent.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, CompletePolicyData, DenialAuditMode, EffectsData, GovernanceMode,
    PolicySchemaCharge,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::{BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::LimitEnforcement;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::prepare_complete_policy_replace_intent;

use super::super::{PRESENT_INTENT_BYTES, TestResult, input, present_data, profile};

/// Set one independent version and its frozen local wire replacement.
fn set_intent_version_case(
    data: &mut CompletePolicyData,
    literal: &mut Vec<u8>,
    case: usize,
) -> TestResult {
    data.version = match case {
        0 => None,
        1 => Some(PolicyVersion::Bytes(Vec::new())),
        2 => Some(PolicyVersion::Bytes(vec![0, 0xff])),
        3 => Some(PolicyVersion::Number(0)),
        4 => Some(PolicyVersion::Number(u64::MAX)),
        _ => return Err("unknown version case".into()),
    };
    let bytes: &[u8] = match case {
        0 => b"\0",
        1 => b"\x01\0\0\0\0\0",
        2 => b"\x01\0\0\0\0\x02\0\xff",
        3 => b"\x01\x01\0\0\0\0\0\0\0\0",
        4 => b"\x01\x01\xff\xff\xff\xff\xff\xff\xff\xff",
        _ => return Err("unknown version case".into()),
    };
    *literal = PRESENT_INTENT_BYTES[..41].to_vec();
    literal.extend_from_slice(bytes);
    literal.extend_from_slice(&PRESENT_INTENT_BYTES[51..]);
    Ok(())
}

#[test]
fn complete_replace_intent_encodes_scalar_variants_and_widths() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for case in 0..21 {
        let (mut value, mut expected) = (present_data(), present_data());
        let mut literal = PRESENT_INTENT_BYTES.to_vec();
        for data in [&mut value, &mut expected] {
            let retry = data.retry.as_mut().ok_or("missing retry")?;
            let backoff = retry.backoff.as_mut().ok_or("missing backoff")?;
            let retention = data.retention.as_mut().ok_or("missing retention")?;
            let state = data
                .states
                .get_mut(b"s".as_slice())
                .ok_or("missing state")?;
            let gov = data.governance.as_mut().ok_or("missing governance")?;
            let effects = gov.effects.as_mut().ok_or("missing effects")?;
            let audit = gov.audit.as_mut().ok_or("missing audit")?;
            let limits_map = gov.limits.as_mut().ok_or("missing limits")?;
            let limit = limits_map.get_mut(b"l".as_slice()).ok_or("missing limit")?;
            let budgets = gov.budgets.as_mut().ok_or("missing budgets")?;
            let budget = budgets.get_mut(b"b".as_slice()).ok_or("missing budget")?;
            match case {
                0..=4 => set_intent_version_case(data, &mut literal, case)?,
                5..=8 => {
                    backoff.kind = [
                        BackoffKind::None,
                        BackoffKind::Fixed,
                        BackoffKind::Linear,
                        BackoffKind::Exponential,
                    ][case - 5];
                    literal[65] = [0, 1, 2, 3][case - 5];
                }
                9..=10 => {
                    state.mode = Some([StateMode::Parallel, StateMode::Fifo][case - 9]);
                    literal[114] = [0, 1][case - 9];
                }
                11..=13 => {
                    gov.mode = Some(
                        [
                            GovernanceMode::Minimal,
                            GovernanceMode::Ledger,
                            GovernanceMode::Full,
                        ][case - 11],
                    );
                    literal[165] = [0, 1, 2][case - 11];
                }
                14..=16 => {
                    audit.denials = Some(
                        [
                            DenialAuditMode::Off,
                            DenialAuditMode::Sampled,
                            DenialAuditMode::All,
                        ][case - 14],
                    );
                    literal[193] = [0, 1, 2][case - 14];
                }
                17..=18 => {
                    let enforcement = [
                        LimitEnforcement::StrictGlobal,
                        LimitEnforcement::ApproximateGlobal,
                    ][case - 17];
                    limit.enforcement = Some(enforcement);
                    budget.enforcement = Some(enforcement);
                    literal[213] = [0, 1][case - 17];
                    literal[250] = [0, 1][case - 17];
                }
                19 => {
                    data.max_active_ms = Some(0x0102_0304_0506_0708);
                    retry.max_retries = Some(0x0102);
                    backoff.base_ms = 0x0102_0304_0506_0708;
                    backoff.max_ms = 0x1112_1314_1516_1718;
                    backoff.jitter_pct = 100;
                    retention.ttl_ms = Some(0x2122_2324_2526_2728);
                    retention.history_max_events = Some(0x0102_0304);
                    set_intent_width_literal(&mut literal);
                }
                _ => {
                    effects.require_idempotency_key = Some(true);
                    audit.effect_events = Some(true);
                    literal[188] = 1;
                    literal[191] = 1;
                }
            }
        }
        let result = prepare_complete_policy_replace_intent(
            input(value, None)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert!(result.caller().input().data() == &expected);
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        assert_eq!(result.digest(), &digest);
    }
    Ok(())
}

#[test]
fn complete_replace_intent_preserves_local_empty_presence() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let cases = local_empty_intent_cases();
    for (case, start, end, empty) in cases {
        for present in [false, true] {
            let (mut value, mut expected) = (present_data(), present_data());
            for data in [&mut value, &mut expected] {
                configure_empty_data(data, case, present)?;
            }
            let replacement = if present { empty } else { b"\0" };
            let mut literal = PRESENT_INTENT_BYTES[..start].to_vec();
            literal.extend_from_slice(replacement);
            literal.extend_from_slice(&PRESENT_INTENT_BYTES[end..]);
            let result = prepare_complete_policy_replace_intent(
                input(value, None)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            assert!(result.caller().input().data() == &expected);
            let digest: [u8; 32] = Sha256::digest(&literal).into();
            assert_eq!(result.digest(), &digest);
        }
    }
    Ok(())
}

fn set_intent_width_literal(literal: &mut [u8]) {
    literal[52..60].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    literal[62..64].copy_from_slice(&[1, 2]);
    literal[66..74].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    literal[74..82].copy_from_slice(&[17, 18, 19, 20, 21, 22, 23, 24]);
    literal[82] = 100;
    literal[91..99].copy_from_slice(&[33, 34, 35, 36, 37, 38, 39, 40]);
    literal[100..104].copy_from_slice(&[1, 2, 3, 4]);
}

fn local_empty_intent_cases() -> [(usize, usize, usize, &'static [u8]); 20] {
    [
        (0, 41, 51, b"\x01\0\0\0\0\0"),        // version Bytes
        (1, 83, 89, b"\x01\0\0\0\0"),          // retry target
        (2, 157, 163, b"\x01\0\0\0\0"),        // state metadata
        (3, 260, 266, b"\x01\0\0\0\0"),        // budget unit
        (4, 142, 157, b"\x01\0\0\0\0"),        // attributes
        (5, 167, 182, b"\x01\0\0\0\0"),        // allowed effects
        (6, 182, 187, b"\x01\0\0\0\0"),        // denied effects
        (7, 339, 354, b"\x01\0\0\0\0"),        // circuit classes
        (8, 355, 365, b"\x01\0\0\0\0"),        // approval effects
        (9, 365, 375, b"\x01\0\0\0\0"),        // approval states
        (10, 375, 385, b"\x01\0\0\0\0"),       // approval assignees
        (11, 194, 223, b"\x01\0\0\0\0"),       // limits map
        (12, 223, 266, b"\x01\0\0\0\0"),       // budgets map
        (13, 266, 354, b"\x01\0\0\0\0"),       // circuits map
        (14, 166, 189, b"\x01\0\0\0"),         // Effects group
        (15, 189, 194, b"\x01\0\0"),           // Audit group
        (16, 354, 394, b"\x01\0\0\0\0"),       // Approvals group
        (17, 115, 119, b"\x01\0\0\0"),         // state Retry
        (18, 119, 134, b"\x01\0\0"),           // state Retention
        (19, 134, 142, b"\x01\0\0\0\0\0\0\0"), // state Gov
    ]
}

fn configure_empty_data(data: &mut CompletePolicyData, case: usize, present: bool) -> TestResult {
    let retry = data.retry.as_mut().ok_or("missing retry")?;
    let state = data
        .states
        .get_mut(b"s".as_slice())
        .ok_or("missing state")?;
    let gov = data.governance.as_mut().ok_or("missing governance")?;
    let effects = gov.effects.as_mut().ok_or("missing effects")?;
    let approvals = gov.approvals.as_mut().ok_or("missing approvals")?;
    let budgets = gov.budgets.as_mut().ok_or("missing budgets")?;
    let budget = budgets.get_mut(b"b".as_slice()).ok_or("missing budget")?;
    let circuits = gov.circuits.as_mut().ok_or("missing circuits")?;
    let circuit = circuits.get_mut(b"c".as_slice()).ok_or("missing circuit")?;
    match case {
        0 => data.version = present.then_some(PolicyVersion::Bytes(Vec::new())),
        1 => retry.exhausted_to = present.then_some(Vec::new()),
        2 => data.indexed_state_meta = present.then_some(Vec::new()),
        3 => budget.unit = present.then_some(Vec::new()),
        4 => data.indexed_attributes = present.then_some(Vec::new()),
        5 => effects.allowed = present.then_some(Vec::new()),
        6 => effects.denied = present.then_some(Vec::new()),
        7 => circuit.error_classes = present.then_some(Vec::new()),
        8 => approvals.effects = present.then_some(Vec::new()),
        9 => approvals.states = present.then_some(Vec::new()),
        10 => approvals.assignees = present.then_some(Vec::new()),
        11 => gov.limits = present.then_some(BTreeMap::new()),
        12 => gov.budgets = present.then_some(BTreeMap::new()),
        13 => gov.circuits = present.then_some(BTreeMap::new()),
        14 => {
            gov.effects = present.then_some(EffectsData {
                allowed: None,
                denied: None,
                require_idempotency_key: None,
            });
        }
        15 => {
            gov.audit = present.then_some(AuditData {
                effect_events: None,
                denials: None,
            });
        }
        16 => {
            gov.approvals = present.then_some(ApprovalsData {
                effects: None,
                states: None,
                assignees: None,
                timeout_ms: None,
            });
        }
        17 => state.retry = present.then_some(RetryOverride::default()),
        18 => state.retention = present.then_some(RetentionOverride::default()),
        19 => {
            if !present {
                state.governance = None;
            }
        }
        _ => return Err("unknown empty-field case".into()),
    }
    Ok(())
}
