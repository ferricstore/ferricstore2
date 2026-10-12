//! Actual decoded headers preserve fresh rejection and retained replay semantics.

use std::collections::BTreeMap;

use wf_domain::policy::installation::PolicyError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::policy::{PolicyRequestView, decide_policy_request};
use wf_domain::workflow::request::{
    DispatchDecision, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{CodecLimits, decode_prepared, encode_prepared};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

use super::{TestResult, request};

#[test]
fn actual_decoded_policy_header_supports_fresh_and_retained_replay() -> TestResult {
    let request = request("policy_empty")?;
    let encoded = encode_prepared(&request, CodecLimits::default())?;
    let decoded = decode_prepared(encoded.as_bytes(), CodecLimits::default())?;
    let scope = &decoded.request().scope;
    let initial = plan_control(
        ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?),
        EntryIdentity::new(
            LogIdentity::new(0, 0, AppliedPosition::new(0)),
            PreparedDigest::new([0; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next();
    let outcomes = BTreeMap::new();
    let envelope = decoded
        .policy_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
        .ok_or("missing envelope")?;
    let view = PolicyRequestView {
        gate: GateView {
            scope,
            control: initial,
            outcomes: &outcomes,
        },
        existing: None,
        migration_high_water: 0,
    };
    let first = decide_policy_request(
        view,
        envelope,
        CodecLimits::default().policy_limits(),
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(first.decision(), DispatchDecision::Fresh(Ok(plan)) if plan.value().encoded() == envelope.prepared.encoded())
    );
    assert!(first.requires_outcome_write());
    let original = RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: request.request.clone(),
            intent: request.intent,
            original_position: envelope.header.identity.log(),
            original_time: TimestampMillis::new(11),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::Accepted,
        },
        1,
        b"original-policy-result".to_vec(),
        AdmissionLimits::default(),
    )?;
    let outcomes = BTreeMap::from([(request.request.clone(), original.clone())]);
    let replay = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope,
                control: first.control().next(),
                outcomes: &outcomes,
            },
            existing: None,
            migration_high_water: 77,
        },
        decoded
            .policy_envelope(LogIdentity::new(2, 9, AppliedPosition::new(2)))
            .ok_or("missing envelope")?,
        CodecLimits::default().policy_limits(),
        AdmissionLimits::default(),
    )?;
    assert!(matches!(replay.decision(), DispatchDecision::Replay(value) if *value == &original));
    assert!(!replay.requires_outcome_write());
    assert_eq!(outcomes[&request.request], original);
    assert_eq!(replay.control().next().position().get(), 2);
    Ok(())
}

#[test]
fn actual_decoded_stale_policy_keeps_control_and_requires_a_rejection_outcome() -> TestResult {
    let mut request = request("policy_empty")?;
    let PreparedOperation::InstallPolicy { command, .. } = &mut request.operation else {
        return Err("missing policy operation".into());
    };
    command.expected_generation = Some(99);
    let limits = CodecLimits::default();
    let encoded = encode_prepared(&request, limits)?;
    let decoded = decode_prepared(encoded.as_bytes(), limits)?;
    let scope = &decoded.request().scope;
    let initial = plan_control(
        ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?),
        EntryIdentity::new(
            LogIdentity::new(0, 0, AppliedPosition::new(0)),
            PreparedDigest::new([0; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next();
    let outcomes = BTreeMap::new();
    let result = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope,
                control: initial,
                outcomes: &outcomes,
            },
            existing: None,
            migration_high_water: 0,
        },
        decoded
            .policy_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
            .ok_or("missing envelope")?,
        limits.policy_limits(),
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(result.decision(), DispatchDecision::Fresh(Err(error)) if error.error() == PolicyError::StaleGeneration)
    );
    assert!(result.requires_outcome_write());
    assert_eq!(result.control().next().position().get(), 1);
    assert_eq!(result.control().execution_time().get(), 11);
    assert_eq!(
        result.control().next().time().hlc(),
        HlcTimestamp::new(11, 2)?
    );
    assert_eq!(result.control().next().time().wall().get(), 7);
    assert_eq!(initial.position().get(), 0);
    assert!(outcomes.is_empty());
    Ok(())
}

#[test]
fn actual_decoded_mode_null_is_a_fresh_business_rejection_without_policy_mutation() -> TestResult {
    let installed = request("policy_replace")?;
    let PreparedOperation::InstallPolicy {
        value: existing, ..
    } = &installed.operation
    else {
        return Err("missing installed policy fixture".into());
    };
    let before = existing.clone();
    let request = request("policy_clear")?;
    let limits = CodecLimits::default();
    let encoded = encode_prepared(&request, limits)?;
    let decoded = decode_prepared(encoded.as_bytes(), limits)?;
    let scope = &decoded.request().scope;
    let initial = plan_control(
        ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?),
        EntryIdentity::new(
            LogIdentity::new(0, 0, AppliedPosition::new(0)),
            PreparedDigest::new([0; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next();
    let outcomes = BTreeMap::new();
    let result = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope,
                control: initial,
                outcomes: &outcomes,
            },
            existing: Some(existing),
            migration_high_water: 0,
        },
        decoded
            .policy_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
            .ok_or("missing envelope")?,
        limits.policy_limits(),
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(result.decision(), DispatchDecision::Fresh(Err(error))
        if error.error() == PolicyError::InvalidStateMode)
    );
    assert!(result.requires_outcome_write());
    assert_eq!(result.control().next().position().get(), 1);
    assert_eq!(result.control().execution_time().get(), 11);
    assert_eq!(
        result.control().next().time().hlc(),
        HlcTimestamp::new(11, 2)?
    );
    assert_eq!(result.control().next().time().wall().get(), 7);
    assert_eq!(initial.position().get(), 0);
    assert!(outcomes.is_empty());
    assert_eq!(existing, &before);
    assert_eq!(existing.generation(), 1);
    Ok(())
}

#[test]
fn actual_decoded_replacement_null_state_is_a_fresh_business_rejection() -> TestResult {
    let mut request = request("policy_empty")?;
    let PreparedOperation::InstallPolicy { command, .. } = &mut request.operation else {
        return Err("missing policy operation".into());
    };
    command.mode = wf_domain::policy::installation::InstallMode::Replace;
    command.patch.states.insert(
        vec![0, 255],
        wf_domain::policy::definition::StateChange::Remove,
    );
    let limits = CodecLimits::default();
    let encoded = encode_prepared(&request, limits)?;
    let decoded = decode_prepared(encoded.as_bytes(), limits)?;
    let scope = &decoded.request().scope;
    let initial = plan_control(
        ControlState::initial(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?),
        EntryIdentity::new(
            LogIdentity::new(0, 0, AppliedPosition::new(0)),
            PreparedDigest::new([0; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next();
    let outcomes = BTreeMap::new();
    let result = decide_policy_request(
        PolicyRequestView {
            gate: GateView {
                scope,
                control: initial,
                outcomes: &outcomes,
            },
            existing: None,
            migration_high_water: 0,
        },
        decoded
            .policy_envelope(LogIdentity::new(1, 7, AppliedPosition::new(1)))
            .ok_or("missing envelope")?,
        limits.policy_limits(),
        AdmissionLimits::default(),
    )?;
    assert!(
        matches!(result.decision(), DispatchDecision::Fresh(Err(error))
        if error.error() == PolicyError::InvalidStatePolicy)
    );
    assert!(result.requires_outcome_write());
    assert_eq!(result.control().next().position().get(), 1);
    assert_eq!(result.control().execution_time().get(), 11);
    assert_eq!(
        result.control().next().time().hlc(),
        HlcTimestamp::new(11, 2)?
    );
    assert_eq!(result.control().next().time().wall().get(), 7);
    assert_eq!(initial.position().get(), 0);
    assert!(outcomes.is_empty());
    Ok(())
}
