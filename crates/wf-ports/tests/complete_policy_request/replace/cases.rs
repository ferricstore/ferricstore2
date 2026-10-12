//! Typed fixture edits and per-list canonicality controls.

use std::collections::BTreeMap;

use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, CompletePolicyData, CompletePolicyEncodingError,
    CompletePolicyReplaceInput, CompletePolicyRequestError, DenialAuditMode, EffectsData,
    GovernanceData, GovernanceMode, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::LimitEnforcement;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{prepare_complete_policy_replace_frame, verify_complete_policy_replace_frame};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::{PRESENT_INTENT_BYTES, TestResult, minimal_data, present_data, profile};
use super::literals::{assert_replace_frame_list_order, replace_frame_literal};

pub(super) fn replace_frame_input(
    data: CompletePolicyData,
    guard: Option<u64>,
    ids: [&[u8]; 4],
    stamp: CommittedTime,
) -> TestResult<CompletePolicyReplaceInput> {
    Ok(CompletePolicyReplaceInput::new(
        RequestScope::new(
            TenantId::new(ids[0].to_vec())?,
            GroupId::new(ids[1].to_vec())?,
            RecoveryId::new(ids[2].to_vec())?,
        ),
        RequestId::new(ids[3].to_vec())?,
        stamp,
        guard,
        data,
    ))
}

pub(super) fn replace_frame_empty_field(
    data: &mut CompletePolicyData,
    case: usize,
    present: bool,
) -> TestResult {
    let retry = data.retry.as_mut().ok_or("retry fixture")?;
    let state = data
        .states
        .get_mut(b"s".as_slice())
        .ok_or("state fixture")?;
    let governance = data.governance.as_mut().ok_or("governance fixture")?;
    let effects = governance.effects.as_mut().ok_or("effects fixture")?;
    let approvals = governance.approvals.as_mut().ok_or("approvals fixture")?;
    match case {
        0 => data.version = present.then(|| PolicyVersion::Bytes(Vec::new())),
        1 => retry.exhausted_to = present.then(Vec::new),
        2 => data.indexed_state_meta = present.then(Vec::new),
        3 => {
            governance
                .budgets
                .as_mut()
                .and_then(|rules| rules.get_mut(b"b".as_slice()))
                .ok_or("budget fixture")?
                .unit = present.then(Vec::new);
        }
        4 => data.indexed_attributes = present.then(Vec::new),
        5 => effects.allowed = present.then(Vec::new),
        6 => effects.denied = present.then(Vec::new),
        7 => {
            governance
                .circuits
                .as_mut()
                .and_then(|rules| rules.get_mut(b"c".as_slice()))
                .ok_or("circuit fixture")?
                .error_classes = present.then(Vec::new);
        }
        8 => approvals.effects = present.then(Vec::new),
        9 => approvals.states = present.then(Vec::new),
        10 => approvals.assignees = present.then(Vec::new),
        11 => governance.limits = present.then(BTreeMap::new),
        12 => governance.budgets = present.then(BTreeMap::new),
        13 => governance.circuits = present.then(BTreeMap::new),
        14 => {
            governance.effects = present.then_some(EffectsData {
                allowed: None,
                denied: None,
                require_idempotency_key: None,
            });
        }
        15 => {
            governance.audit = present.then_some(AuditData {
                effect_events: None,
                denials: None,
            });
        }
        16 => {
            governance.approvals = present.then_some(ApprovalsData {
                effects: None,
                states: None,
                assignees: None,
                timeout_ms: None,
            });
        }
        17 => state.retry = present.then(RetryOverride::default),
        18 => state.retention = present.then(RetentionOverride::default),
        19 => {
            state.governance = if present {
                state.governance.take()
            } else {
                None
            }
        }
        20 => data.retry = present.then(RetryOverride::default),
        21 => data.retention = present.then(RetentionOverride::default),
        22 => {
            data.governance = present.then_some(GovernanceData {
                mode: None,
                effects: None,
                audit: None,
                limits: None,
                budgets: None,
                circuits: None,
                approvals: None,
            });
        }
        _ => return Err("unknown independent presence fixture".into()),
    }
    Ok(())
}

pub(super) fn replace_frame_widths(data: &mut CompletePolicyData, maximum: bool) -> TestResult {
    let scalar = if maximum { u64::MAX } else { 0 };
    data.version = Some(PolicyVersion::Number(scalar));
    data.max_active_ms = Some(scalar);
    let retry = data.retry.as_mut().ok_or("retry fixture")?;
    retry.max_retries = Some(if maximum { u16::MAX } else { 0 });
    let backoff = retry.backoff.as_mut().ok_or("backoff fixture")?;
    backoff.base_ms = scalar;
    backoff.max_ms = scalar;
    backoff.jitter_pct = if maximum { 255 } else { 0 };
    data.retention = Some(RetentionOverride {
        ttl_ms: Some(scalar),
        history_max_events: Some(if maximum { u32::MAX } else { 0 }),
    });
    let state = data
        .states
        .get_mut(b"s".as_slice())
        .ok_or("state fixture")?;
    state.mode = Some(if maximum {
        StateMode::Fifo
    } else {
        StateMode::Parallel
    });
    state.retention = data.retention;
    let governance = data.governance.as_mut().ok_or("governance fixture")?;
    governance
        .effects
        .as_mut()
        .ok_or("effects fixture")?
        .require_idempotency_key = Some(maximum);
    governance.mode = Some(if maximum {
        GovernanceMode::Full
    } else {
        GovernanceMode::Minimal
    });
    governance.audit = Some(AuditData {
        effect_events: Some(maximum),
        denials: Some(if maximum {
            DenialAuditMode::All
        } else {
            DenialAuditMode::Off
        }),
    });
    let limit = governance
        .limits
        .as_mut()
        .and_then(|rules| rules.get_mut(b"l".as_slice()))
        .ok_or("limit fixture")?;
    limit.limit = scalar;
    limit.lease_size = Some(scalar);
    limit.enforcement = Some(if maximum {
        LimitEnforcement::ApproximateGlobal
    } else {
        LimitEnforcement::StrictGlobal
    });
    let budget = governance
        .budgets
        .as_mut()
        .and_then(|rules| rules.get_mut(b"b".as_slice()))
        .ok_or("budget fixture")?;
    budget.limit = scalar;
    budget.window_ms = scalar;
    budget.lease_size = Some(scalar);
    budget.enforcement = limit.enforcement;
    let circuit = governance
        .circuits
        .as_mut()
        .and_then(|rules| rules.get_mut(b"c".as_slice()))
        .ok_or("circuit fixture")?;
    for value in [&mut circuit.failure_threshold, &mut circuit.open_ms] {
        *value = scalar;
    }
    for value in [
        &mut circuit.window_ms,
        &mut circuit.min_calls,
        &mut circuit.latency_threshold_ms,
        &mut circuit.half_open_max_probes,
        &mut circuit.half_open_success_threshold,
    ] {
        *value = Some(scalar);
    }
    circuit.failure_rate_pct = Some(if maximum { 255 } else { 0 });
    governance
        .approvals
        .as_mut()
        .ok_or("approvals fixture")?
        .timeout_ms = Some(scalar);
    Ok(())
}

pub(super) fn replace_frame_raw_field(
    data: &mut CompletePolicyData,
    field: usize,
    payload: Vec<u8>,
) -> TestResult {
    let governance = data.governance.as_mut().ok_or("governance fixture")?;
    match field {
        0 => data.workflow_type = payload,
        1 => data.version = Some(PolicyVersion::Bytes(payload)),
        2 => data.retry.as_mut().ok_or("retry fixture")?.exhausted_to = Some(payload),
        3 => {
            let state = data.states.pop_first().ok_or("state fixture")?.1;
            data.states.insert(payload, state);
        }
        4 => data.indexed_attributes.as_mut().ok_or("indexed fixture")?[0] = payload,
        5 => data.indexed_state_meta = Some(payload),
        6 => {
            governance
                .effects
                .as_mut()
                .and_then(|effects| effects.allowed.as_mut())
                .ok_or("allowed fixture")?[0] = payload;
        }
        7 => {
            governance
                .budgets
                .as_mut()
                .and_then(|rules| rules.get_mut(b"b".as_slice()))
                .ok_or("budget fixture")?
                .unit = Some(payload);
        }
        8 => {
            governance
                .circuits
                .as_mut()
                .and_then(|rules| rules.get_mut(b"c".as_slice()))
                .and_then(|rule| rule.error_classes.as_mut())
                .ok_or("classes fixture")?[0] = payload;
        }
        9 => {
            let rules = governance.limits.as_mut().ok_or("limit fixture")?;
            let rule = rules.pop_first().ok_or("limit rule fixture")?.1;
            rules.insert(payload, rule);
        }
        10 => {
            let rules = governance.budgets.as_mut().ok_or("budget fixture")?;
            let rule = rules.pop_first().ok_or("budget rule fixture")?.1;
            rules.insert(payload, rule);
        }
        11 => {
            let rules = governance.circuits.as_mut().ok_or("circuit fixture")?;
            let rule = rules.pop_first().ok_or("circuit rule fixture")?.1;
            rules.insert(payload, rule);
        }
        12 => {
            governance
                .approvals
                .as_mut()
                .and_then(|value| value.effects.as_mut())
                .ok_or("approval effects fixture")?[0] = payload;
        }
        13 => {
            governance
                .approvals
                .as_mut()
                .and_then(|value| value.states.as_mut())
                .ok_or("approval states fixture")?[0] = payload;
        }
        14 => {
            governance
                .approvals
                .as_mut()
                .and_then(|value| value.assignees.as_mut())
                .ok_or("approval assignees fixture")?[0] = payload;
        }
        _ => return Err("unknown RAW fixture".into()),
    }
    Ok(())
}

pub(super) fn replace_frame_governance_list(
    data: &mut CompletePolicyData,
    site: usize,
    state: bool,
) -> TestResult {
    if state {
        data.states
            .get_mut(b"s".as_slice())
            .ok_or("state fixture")?
            .governance = present_data().governance;
    }
    let governance = if state {
        data.states
            .get_mut(b"s".as_slice())
            .and_then(|value| value.governance.as_mut())
    } else {
        data.governance.as_mut()
    }
    .ok_or("governance fixture")?;
    if site == 6 {
        data.indexed_attributes
            .as_mut()
            .ok_or("indexed fixture")?
            .reverse();
        governance
            .effects
            .as_mut()
            .and_then(|effects| effects.allowed.as_mut())
            .ok_or("effects fixture")?
            .reverse();
        governance
            .circuits
            .as_mut()
            .and_then(|rules| rules.get_mut(b"c".as_slice()))
            .and_then(|rule| rule.error_classes.as_mut())
            .ok_or("classes fixture")?
            .reverse();
        let approvals = governance.approvals.as_mut().ok_or("approvals fixture")?;
        approvals.states = Some(vec![b"a".to_vec()]);
        approvals.assignees = Some(vec![b"a".to_vec()]);
        return Ok(());
    }
    let names = match site {
        0 => {
            &mut governance
                .effects
                .as_mut()
                .ok_or("effects fixture")?
                .allowed
        }
        1 => &mut governance.effects.as_mut().ok_or("effects fixture")?.denied,
        2 => {
            &mut governance
                .circuits
                .as_mut()
                .and_then(|rules| rules.get_mut(b"c".as_slice()))
                .ok_or("circuit fixture")?
                .error_classes
        }
        3 => {
            &mut governance
                .approvals
                .as_mut()
                .ok_or("approvals fixture")?
                .effects
        }
        4 => {
            &mut governance
                .approvals
                .as_mut()
                .ok_or("approvals fixture")?
                .states
        }
        5 => {
            &mut governance
                .approvals
                .as_mut()
                .ok_or("approvals fixture")?
                .assignees
        }
        _ => return Err("unknown list fixture".into()),
    };
    *names = Some(vec![b"a".to_vec(), b"a".to_vec()]);
    Ok(())
}

pub(super) fn assert_replace_frame_canonical_lists(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for state in [false, true] {
        for (site, (start, end)) in [
            (167, 182),
            (182, 187),
            (339, 354),
            (355, 365),
            (365, 375),
            (375, 385),
        ]
        .into_iter()
        .enumerate()
        {
            let mut data = present_data();
            let mut intent = PRESENT_INTENT_BYTES.to_vec();
            replace_frame_governance_list(&mut data, site, state)?;
            let base = if state {
                drop(intent.splice(134..142, PRESENT_INTENT_BYTES[163..].iter().copied()));
                134
            } else {
                163
            };
            let start = base + start - 163;
            let end = base + end - 163;
            drop(intent.splice(
                start..end,
                [1, 0, 0, 0, 2, 0, 0, 0, 1, b'a', 0, 0, 0, 1, b'a'],
            ));
            let encoded = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let result = prepare_complete_policy_replace_frame(
                super::super::input(data, None)?,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            assert_eq!(
                result.err().ok_or("producer duplicate")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
            );
            let result = verify_complete_policy_replace_frame(
                &encoded,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            assert_eq!(
                result.err().ok_or("verifier duplicate")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
            );
        }
    }
    let mut intent = super::super::MINIMAL_INTENT_BYTES.to_vec();
    drop(intent.splice(49..50, [1, 0, 0, 0, 2, 0, 0, 0, 1, b'a', 0, 0, 0, 1, b'a']));
    let encoded = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
    assert_eq!(
        verify_complete_policy_replace_frame(&encoded, limits, AdmissionLimits::default(), charge,)
            .err()
            .ok_or("indexed duplicate verifier")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
    );
    assert_replace_frame_list_order(limits, charge)?;
    assert_replace_frame_duplicate_priority()?;
    Ok(())
}

pub(super) fn assert_replace_frame_duplicate_priority() -> TestResult {
    for (maximum, frame_cap, expected_g, expected_p, error) in [
        (
            2_967,
            149,
            2_839,
            128,
            CompletePolicyEncodingError::Malformed,
        ),
        (
            2_966,
            149,
            2_710,
            128,
            CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes {
                bytes: 2_967,
                maximum: 2_966,
            }),
        ),
        (2_325, 148, 2_325, 0, CompletePolicyEncodingError::ByteLimit),
        (
            2_708,
            149,
            2_581,
            0,
            CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes {
                bytes: 2_709,
                maximum: 2_708,
            }),
        ),
    ] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mut data = minimal_data();
        data.indexed_attributes = Some(vec![b"a".to_vec(), b"a".to_vec()]);
        let result = prepare_complete_policy_replace_frame(
            super::super::input(data, None)?,
            limits,
            AdmissionLimits::new(1_048_576, frame_cap, 1_000)?,
            &mut charge,
        );
        assert_eq!(
            result.err().ok_or("exact duplicate priority")?,
            CompletePolicyRequestError::Encoding(error)
        );
        assert_eq!((charge.g(), charge.p()), (expected_g, expected_p));
    }
    Ok(())
}
