//! Fixed source responses and independently assembled original VALUE owners.

use super::cases::{log, stamp};
use super::{
    CompletePolicyData, CompletePolicyReplaceApplyView, CompletePolicyReplaceRequestPlan,
    DispatchDecision, PolicyError, RetainedOutcome, RetainedStatus, TestResult,
};
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CircuitTrackingFailure as Tracking, CompletePolicyReplaceRejectionKind as Kind,
    GovernanceField as Field, GovernanceRuleKind as Rule,
};
use wf_domain::policy::installation::MAX_POLICY_GENERATION;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};

use super::super::super::{MINIMAL_INTENT_BYTES, PRESENT_INTENT_BYTES};

pub(super) fn value_bytes(generation: u64, present: bool) -> TestResult<Vec<u8>> {
    let fields = if present {
        &PRESENT_INTENT_BYTES[36..]
    } else {
        &MINIMAL_INTENT_BYTES[36..]
    };
    let mut snapshot = b"FWPS\0\x02".to_vec();
    snapshot.extend_from_slice(fields);
    let mut value = b"FWPV\0\x02".to_vec();
    value.extend_from_slice(&generation.to_be_bytes());
    value.extend_from_slice(&u32::try_from(snapshot.len())?.to_be_bytes());
    value.extend_from_slice(&snapshot);
    assert_eq!(value.len(), if present { 382 } else { 40 });
    if generation == 1 {
        let expected = if present {
            "0f61166723f2a7466e67d447059dc25128c8b5aec270a9a626e3edbeeb03b07f"
        } else {
            "cce55bbc22a124e63173e9b7c60e5e8c8c99a1f6847aa8a07750f02e0ff350df"
        };
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&value)).as_slice(),
            super::super::literals::replace_frame_hex(expected)?.as_slice()
        );
    }
    Ok(value)
}

pub(super) fn generation_body(error: PolicyError) -> TestResult<&'static [u8]> {
    Ok(match error {
        PolicyError::InvalidGeneration => b"ERR invalid flow policy generation",
        PolicyError::StaleGeneration => b"ERR stale flow policy generation",
        PolicyError::GenerationExhausted => b"ERR flow policy generation exhausted",
        _ => return Err("not a retained generation reason".into()),
    })
}

pub(super) fn assert_outcome(
    outcome: &RetainedOutcome,
    frame: &[u8],
    expected: &[u8],
    time_and_index: (u64, u64),
) -> TestResult {
    let (time, index) = time_and_index;
    assert_eq!(outcome.scope().tenant().as_bytes(), b"t");
    assert_eq!(outcome.scope().group().as_bytes(), b"g");
    assert_eq!(outcome.scope().recovery().as_bytes(), b"r");
    assert_eq!(outcome.request().as_bytes(), &frame[26..27]);
    assert_eq!(outcome.intent().version(), 2);
    assert_eq!(
        *outcome.intent().digest(),
        <[u8; 32]>::from(Sha256::digest(&frame[83..]))
    );
    assert_eq!(outcome.original_position(), log(index));
    assert_eq!(outcome.original_time().get(), time);
    assert_eq!(
        outcome.expires_at().get(),
        time.checked_add(100).ok_or("expected expiry overflow")?
    );
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(outcome.response(), expected);
    Ok(())
}

pub(super) fn assert_control(
    plan: &CompletePolicyReplaceRequestPlan<'_>,
    view: CompletePolicyReplaceApplyView<'_>,
    frame: &[u8],
    time_and_index: (u64, u64),
    replay: bool,
) -> TestResult {
    let (time, index) = time_and_index;
    let committed = stamp(time)?;
    let identity = EntryIdentity::new(
        log(index),
        PreparedDigest::new(Sha256::digest(frame).into()),
    );
    let expected = if replay {
        view.gate.control
    } else {
        let effective = wf_types::version::CommittedTime::new(
            view.gate.control.time().hlc().max(committed.hlc()),
            view.gate.control.time().wall().max(committed.wall()).get(),
        )?;
        ControlState::restored(effective, identity, EntryStamp::Stamped(committed))?
    };
    assert_eq!(plan.expected_revision(), 7);
    assert_eq!(plan.base_control(), view.gate.control);
    assert_eq!(plan.control().next(), expected);
    assert_eq!(plan.control().is_log_replay(), replay);
    assert_eq!(
        plan.control().execution_time(),
        expected.time().hlc().physical()
    );
    Ok(())
}

pub(super) fn assert_accepted(
    plan: &CompletePolicyReplaceRequestPlan<'_>,
    data: &CompletePolicyData,
    expected: &[u8],
    previous: u64,
    frame: &[u8],
    time_and_index: (u64, u64),
) -> TestResult {
    let DispatchDecision::Fresh((Some((stored, value)), outcome)) = plan.decision() else {
        return Err("expected complete policy and original outcome together".into());
    };
    assert_eq!(*stored, previous);
    assert_eq!(
        value.generation(),
        u64::from_be_bytes(expected[6..14].try_into()?)
    );
    assert!(value.data() == data);
    assert_eq!(value.encoded(), expected);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_outcome(outcome, frame, expected, time_and_index)
}

pub(super) fn business_rows() -> Vec<(Kind, &'static [u8])> {
    let mut rows = core_business_rows();
    rows.extend(governance_business_rows());
    rows.extend(circuit_business_rows());
    rows
}

fn core_business_rows() -> Vec<(Kind, &'static [u8])> {
    vec![
        (Kind::EmptyType, b"ERR flow type must be a non-empty string"),
        (
            Kind::InvalidVersion,
            b"ERR flow policy version must be a non-empty string or non-negative integer",
        ),
        (
            Kind::InvalidMaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        ),
        (
            Kind::RetryMaxRetries,
            b"ERR flow retry max_retries must be between 0 and 1000",
        ),
        (
            Kind::RetryBaseMillis,
            b"ERR flow retry base_ms must be between 0 and 2592000000",
        ),
        (
            Kind::RetryMaxMillis,
            b"ERR flow retry max_ms must be between 0 and 2592000000",
        ),
        (
            Kind::RetryJitterPercent,
            b"ERR flow retry jitter_pct must be between 0 and 100",
        ),
        (
            Kind::RetryExhaustedTo,
            b"ERR flow retry exhausted_to must be a non-empty string",
        ),
        (
            Kind::RetryRunningExhaustion,
            b"ERR flow retry exhausted_to cannot be running",
        ),
        (
            Kind::EmptyState,
            b"ERR flow policy state must be a non-empty string",
        ),
        (
            Kind::RunningState,
            b"ERR flow policy state cannot be running",
        ),
        (
            Kind::RetentionTtl,
            b"ERR flow retention ttl_ms must be between 1 and 31536000000",
        ),
        (
            Kind::IndexedAttributeEmpty,
            b"ERR flow attribute key must not be empty",
        ),
        (
            Kind::IndexedAttributeReserved,
            b"ERR flow attribute key is reserved",
        ),
        (
            Kind::IndexedAttributeUtf8,
            b"ERR flow attribute key must be valid UTF-8",
        ),
        (
            Kind::IndexedStateMetaEmpty,
            b"ERR flow state_meta key must not be empty",
        ),
        (
            Kind::IndexedStateMetaReserved,
            b"ERR flow state_meta key is reserved",
        ),
        (
            Kind::IndexedStateMetaUtf8,
            b"ERR flow state_meta key must be valid UTF-8",
        ),
    ]
}

fn governance_business_rows() -> Vec<(Kind, &'static [u8])> {
    vec![
        (
            Kind::GovernanceEmptyName(Rule::Limits),
            b"ERR flow governance limits names must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyName(Rule::Budgets),
            b"ERR flow governance budgets names must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyName(Rule::Circuits),
            b"ERR flow governance circuits names must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyList(Field::Allowed),
            b"ERR flow governance list values must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyList(Field::Denied),
            b"ERR flow governance list values must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyList(Field::ApprovalEffects),
            b"ERR flow governance list values must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyList(Field::ApprovalStates),
            b"ERR flow governance list values must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyList(Field::Assignees),
            b"ERR flow governance list values must be non-empty strings",
        ),
        (
            Kind::GovernanceEmptyList(Field::ErrorClasses),
            b"ERR flow governance list values must be non-empty strings",
        ),
        (
            Kind::GovernanceLimit,
            b"ERR flow governance limit must be a non-negative integer",
        ),
        (
            Kind::GovernancePositive(Field::LeaseSize),
            b"ERR flow governance lease_size must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::WindowMillis),
            b"ERR flow governance window_ms must be a positive integer",
        ),
    ]
}

fn circuit_business_rows() -> Vec<(Kind, &'static [u8])> {
    vec![
        (
            Kind::GovernancePositive(Field::FailureThreshold),
            b"ERR flow governance failure_threshold must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::OpenMillis),
            b"ERR flow governance open_ms must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::MinCalls),
            b"ERR flow governance min_calls must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::LatencyMillis),
            b"ERR flow governance latency_threshold_ms must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::HalfOpenProbes),
            b"ERR flow governance half_open_max_probes must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::HalfOpenSuccesses),
            b"ERR flow governance half_open_success_threshold must be a positive integer",
        ),
        (
            Kind::GovernancePositive(Field::ApprovalTimeout),
            b"ERR flow governance timeout_ms must be a positive integer",
        ),
        (
            Kind::GovernanceUnit,
            b"ERR flow governance unit must be a non-empty string of at most 65535 bytes",
        ),
        (
            Kind::GovernancePercent,
            b"ERR flow governance failure_rate_pct must be an integer from 1 to 100",
        ),
        (
            Kind::GovernanceTracking(Tracking::MinCallsAbove64),
            b"ERR flow governance circuit min_calls must be <= 64",
        ),
        (
            Kind::GovernanceTracking(Tracking::ThresholdAbove64WithoutRate),
            b"ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct",
        ),
        (
            Kind::GovernanceTracking(Tracking::MissingMinCallsForLargeThreshold),
            b"ERR flow governance circuit min_calls is required when failure_threshold exceeds 64",
        ),
    ]
}

pub(super) fn governance_data(
    data: &mut CompletePolicyData,
    kind: Kind,
    invalid: bool,
) -> TestResult {
    let mut governance = super::super::super::present_governance();
    let number = u64::from(!invalid);
    match kind {
        Kind::GovernanceEmptyName(rule) if invalid => match rule {
            Rule::Limits => {
                let row = governance.limits.as_mut().ok_or("limits")?;
                let value = row.remove(b"l".as_slice()).ok_or("limit")?;
                row.insert(vec![], value);
            }
            Rule::Budgets => {
                let row = governance.budgets.as_mut().ok_or("budgets")?;
                let value = row.remove(b"b".as_slice()).ok_or("budget")?;
                row.insert(vec![], value);
            }
            Rule::Circuits => {
                let row = governance.circuits.as_mut().ok_or("circuits")?;
                let value = row.remove(b"c".as_slice()).ok_or("circuit")?;
                row.insert(vec![], value);
            }
        },
        Kind::GovernanceEmptyList(field) => {
            let leaf = Some(vec![if invalid { vec![] } else { b"v".to_vec() }]);
            match field {
                Field::Allowed => governance.effects.as_mut().ok_or("effects")?.allowed = leaf,
                Field::Denied => governance.effects.as_mut().ok_or("effects")?.denied = leaf,
                Field::ApprovalEffects => {
                    governance.approvals.as_mut().ok_or("approval")?.effects = leaf;
                }
                Field::ApprovalStates => {
                    governance.approvals.as_mut().ok_or("approval")?.states = leaf;
                }
                Field::Assignees => {
                    governance.approvals.as_mut().ok_or("approval")?.assignees = leaf;
                }
                Field::ErrorClasses => {
                    governance
                        .circuits
                        .as_mut()
                        .ok_or("circuits")?
                        .get_mut(b"c".as_slice())
                        .ok_or("circuit")?
                        .error_classes = leaf;
                }
                _ => return Err("not a list field".into()),
            }
        }
        Kind::GovernanceLimit => {
            governance
                .limits
                .as_mut()
                .ok_or("limits")?
                .get_mut(b"l".as_slice())
                .ok_or("limit")?
                .limit = if invalid {
                MAX_POLICY_GENERATION + 1
            } else {
                0
            }
        }
        Kind::GovernancePositive(Field::LeaseSize) => {
            governance
                .limits
                .as_mut()
                .ok_or("limits")?
                .get_mut(b"l".as_slice())
                .ok_or("limit")?
                .lease_size = Some(number);
        }
        Kind::GovernancePositive(Field::WindowMillis) => {
            governance
                .budgets
                .as_mut()
                .ok_or("budgets")?
                .get_mut(b"b".as_slice())
                .ok_or("budget")?
                .window_ms = number;
        }
        Kind::GovernancePositive(Field::ApprovalTimeout) => {
            governance.approvals.as_mut().ok_or("approval")?.timeout_ms = Some(number);
        }
        Kind::GovernanceUnit => {
            governance
                .budgets
                .as_mut()
                .ok_or("budgets")?
                .get_mut(b"b".as_slice())
                .ok_or("budget")?
                .unit = Some(if invalid { vec![] } else { vec![255] });
        }
        Kind::GovernanceEmptyName(_) => {}
        _ => circuit_data(&mut governance, kind, invalid)?,
    }
    data.governance = Some(governance);
    Ok(())
}

fn circuit_data(
    governance: &mut wf_domain::policy::complete::GovernanceData,
    kind: Kind,
    invalid: bool,
) -> TestResult {
    let row = governance
        .circuits
        .as_mut()
        .ok_or("circuits")?
        .get_mut(b"c".as_slice())
        .ok_or("circuit")?;
    let positive = u64::from(!invalid);
    match kind {
        Kind::GovernancePositive(Field::FailureThreshold) => row.failure_threshold = positive,
        Kind::GovernancePositive(Field::OpenMillis) => row.open_ms = positive,
        Kind::GovernancePositive(Field::MinCalls) => row.min_calls = Some(positive),
        Kind::GovernancePositive(Field::LatencyMillis) => row.latency_threshold_ms = Some(positive),
        Kind::GovernancePositive(Field::HalfOpenProbes) => {
            row.half_open_max_probes = Some(positive);
        }
        Kind::GovernancePositive(Field::HalfOpenSuccesses) => {
            row.half_open_success_threshold = Some(positive);
        }
        Kind::GovernancePercent => row.failure_rate_pct = Some(u8::from(!invalid)),
        Kind::GovernanceTracking(Tracking::MinCallsAbove64) => {
            row.min_calls = Some(if invalid { 65 } else { 64 });
        }
        Kind::GovernanceTracking(Tracking::ThresholdAbove64WithoutRate) => {
            row.failure_threshold = if invalid { 65 } else { 64 };
            row.failure_rate_pct = None;
        }
        Kind::GovernanceTracking(Tracking::MissingMinCallsForLargeThreshold) => {
            row.failure_threshold = 65;
            row.min_calls = if invalid { None } else { Some(64) };
        }
        _ => return Err("not a circuit fixture".into()),
    }
    Ok(())
}
