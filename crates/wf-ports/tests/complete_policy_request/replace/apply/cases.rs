//! Coherent mathematical fixtures, source invalid neighbors and priority controls.

use super::literals::{assert_outcome, business_rows, generation_body, governance_data};
use super::{
    AdmissionLimits, ApplyError, BTreeMap, CompletePolicyData, CompletePolicyEncodingError,
    CompletePolicyReplaceApplyError, CompletePolicyReplaceApplyView, CompletePolicyRequestError,
    CompletePolicyValue, ControlState, DispatchDecision, PolicyError, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, RequestId, RequestScope, RetainedOutcome,
    RetainedStatus, TestResult, apply, minimal_data, profile,
};
use wf_domain::policy::complete::{
    CircuitTrackingFailure as Tracking, CompletePolicyReplaceRejectionKind as Kind,
    CompleteStatePolicyData, GovernanceField as Field, StoredPolicyValue,
    prepare_complete_policy_replace, prepare_complete_policy_value_v2, restore_complete_policy,
};
use wf_domain::policy::definition::{PolicyPatch, PolicyVersion};
use wf_domain::policy::installation::MAX_POLICY_GENERATION;
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::request::gate::GateView;
use wf_types::identity::{GroupId, RecoveryId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}

pub(super) fn stamp(time: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(time, 0)?, time)?)
}

pub(super) const fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

pub(super) fn view<'a>(
    scope: &'a RequestScope,
    control: ControlState,
    outcomes: &'a BTreeMap<RequestId, RetainedOutcome>,
    policies: &'a BTreeMap<Vec<u8>, Result<CompletePolicyValue, ApplyError>>,
    active: &'a BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    done: &'a BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
) -> CompletePolicyReplaceApplyView<'a> {
    CompletePolicyReplaceApplyView {
        gate: GateView {
            scope,
            control,
            outcomes,
        },
        revision: 7,
        policies,
        active_migrations: active,
        completed_migrations: done,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Ok(100),
    }
}

pub(super) fn literal_frame(
    present: bool,
    guard: Option<u64>,
    request: &[u8],
    time: u64,
) -> TestResult<Vec<u8>> {
    let original = if present {
        super::super::super::PRESENT_INTENT_BYTES
    } else {
        super::super::super::MINIMAL_INTENT_BYTES
    };
    let mut intent = original[..35].to_vec();
    match guard {
        Some(value) => {
            intent.push(1);
            intent.extend_from_slice(&value.to_be_bytes());
        }
        None => intent.push(0),
    }
    intent.extend_from_slice(&original[36..]);
    super::super::literals::replace_frame_literal(
        &intent,
        [b"t", b"g", b"r", request],
        stamp(time)?,
    )
}

pub(super) fn input_frame(data: CompletePolicyData, guard: Option<u64>) -> TestResult<Vec<u8>> {
    let limits = profile(200_000_000)?;
    let mut setup = PolicySchemaCharge::new(limits);
    let input = super::super::super::input(data, guard)?;
    Ok(wf_ports::log::prepare_complete_policy_replace_frame(
        input,
        limits,
        AdmissionLimits::default(),
        &mut setup,
    )?
    .as_bytes()
    .to_vec())
}

pub(super) fn current(
    generation: u64,
    data: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyValue> {
    Ok(prepare_complete_policy_value_v2(
        restore_complete_policy(generation, data, limits, charge)?,
        limits,
        charge,
    )?)
}

pub(super) fn schema(error: PolicySchemaError) -> CompletePolicyReplaceApplyError {
    CompletePolicyReplaceApplyError::Request(CompletePolicyRequestError::Encoding(
        CompletePolicyEncodingError::Schema(error),
    ))
}

pub(super) fn assert_generation(
    plan: &super::CompletePolicyReplaceRequestPlan<'_>,
    error: PolicyError,
    frame: &[u8],
) -> TestResult {
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("expected generation rejection without a policy write".into());
    };
    let RetainedStatus::PolicyRejected(reason) = outcome.status() else {
        return Err("expected original generation status".into());
    };
    assert_eq!(reason.error(), error);
    assert_outcome(outcome, frame, generation_body(error)?, (0, 0))
}

pub(super) fn state() -> CompleteStatePolicyData {
    CompleteStatePolicyData {
        mode: None,
        retry: None,
        retention: None,
        governance: None,
    }
}

pub(super) fn business_data(kind: Kind, invalid: bool) -> TestResult<CompletePolicyData> {
    let mut data = minimal_data();
    match kind {
        Kind::EmptyType if invalid => data.workflow_type.clear(),
        Kind::InvalidVersion => {
            data.version = Some(PolicyVersion::Bytes(if invalid {
                vec![]
            } else {
                vec![255]
            }));
        }
        Kind::InvalidMaxActive => data.max_active_ms = Some(u64::from(!invalid)),
        Kind::RetryMaxRetries
        | Kind::RetryBaseMillis
        | Kind::RetryMaxMillis
        | Kind::RetryJitterPercent
        | Kind::RetryExhaustedTo
        | Kind::RetryRunningExhaustion => {
            let mut retry = RetryOverride::default();
            match kind {
                Kind::RetryMaxRetries => {
                    retry.max_retries = Some(if invalid { 1001 } else { 1000 });
                }
                Kind::RetryExhaustedTo | Kind::RetryRunningExhaustion => {
                    retry.exhausted_to = Some(if !invalid {
                        b"dead".to_vec()
                    } else if kind == Kind::RetryExhaustedTo {
                        vec![]
                    } else {
                        b"running".to_vec()
                    });
                }
                _ => {
                    retry.backoff = Some(Backoff {
                        kind: BackoffKind::Fixed,
                        base_ms: if kind == Kind::RetryBaseMillis && invalid {
                            2_592_000_001
                        } else {
                            0
                        },
                        max_ms: if kind == Kind::RetryMaxMillis && invalid {
                            2_592_000_001
                        } else {
                            0
                        },
                        jitter_pct: if kind == Kind::RetryJitterPercent && invalid {
                            101
                        } else {
                            100
                        },
                    });
                }
            }
            data.retry = Some(retry);
        }
        Kind::EmptyState | Kind::RunningState => {
            let name = if !invalid {
                b"s".to_vec()
            } else if kind == Kind::EmptyState {
                vec![]
            } else {
                b"running".to_vec()
            };
            data.states.insert(name, state());
        }
        Kind::RetentionTtl => {
            data.retention = Some(RetentionOverride {
                ttl_ms: Some(u64::from(!invalid)),
                history_max_events: None,
            });
        }
        Kind::IndexedAttributeEmpty
        | Kind::IndexedAttributeReserved
        | Kind::IndexedAttributeUtf8
        | Kind::IndexedStateMetaEmpty
        | Kind::IndexedStateMetaReserved
        | Kind::IndexedStateMetaUtf8 => indexed_data(&mut data, kind, invalid),
        Kind::EmptyType => {}
        _ => governance_data(&mut data, kind, invalid)?,
    }
    Ok(data)
}

fn indexed_data(data: &mut CompletePolicyData, kind: Kind, invalid: bool) {
    let bytes = if invalid {
        match kind {
            Kind::IndexedAttributeEmpty => vec![],
            Kind::IndexedStateMetaEmpty => b" \t".to_vec(),
            Kind::IndexedAttributeReserved | Kind::IndexedStateMetaReserved => b"__key".to_vec(),
            _ => vec![255],
        }
    } else {
        b"key".to_vec()
    };
    if matches!(
        kind,
        Kind::IndexedAttributeEmpty | Kind::IndexedAttributeReserved | Kind::IndexedAttributeUtf8
    ) {
        data.indexed_attributes = Some(vec![bytes]);
    } else {
        data.indexed_state_meta = Some(bytes);
    }
}

#[test]
fn complete_replace_parent_preserves_envelope_cas_type_and_old_api_priorities() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let control = ControlState::initial(stamp(0)?);
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    for expected in [Some(0), Some(MAX_POLICY_GENERATION + 1)] {
        let candidate = restore_complete_policy(1, minimal_data(), limits, &mut charge)?;
        let mismatch = candidate.bind_encoded_snapshot(0..1, vec![0], limits, &mut charge)?;
        let policies = BTreeMap::from([(b"r".to_vec(), Ok(mismatch))]);
        let frame = literal_frame(false, expected, b"q", 0)?;
        let mut view = view(&scope, control, &outcomes, &policies, &marks, &marks);
        view.history_max_events_limit = Err(ApplyError::AllocationFailed);
        assert_eq!(
            apply(view, &frame, 0, limits, &mut charge).err(),
            Some(CompletePolicyReplaceApplyError::Request(
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::EncodingMismatch)
            ))
        );
        assert_eq!(
            policies[b"r".as_slice()]
                .as_ref()
                .map(CompletePolicyValue::encoded),
            Ok([0].as_slice())
        );
    }
    let mut foreign = minimal_data();
    foreign.workflow_type = b"foreign".to_vec();
    let foreign_value = current(1, foreign, limits, &mut charge)?;
    let policies = BTreeMap::from([(b"r".to_vec(), Ok(foreign_value))]);
    for expected in [Some(0), Some(1), None] {
        let frame = literal_frame(false, expected, b"q", 0)?;
        let view = view(&scope, control, &outcomes, &policies, &marks, &marks);
        let result = apply(view, &frame, 0, limits, &mut charge);
        if expected == Some(0) {
            assert_generation(&result?, PolicyError::StaleGeneration, &frame)?;
        } else {
            assert_eq!(
                result.err(),
                Some(schema(PolicySchemaError::InvalidAuthority))
            );
        }
    }
    assert_old_priorities(policies, limits, &mut charge)?;
    super::controls::assert_current_caps(limits, &mut charge)?;
    let absent = BTreeMap::new();
    for (guard, error) in [
        (0, None),
        (1, Some(PolicyError::StaleGeneration)),
        (
            MAX_POLICY_GENERATION + 1,
            Some(PolicyError::InvalidGeneration),
        ),
    ] {
        let frame = literal_frame(false, Some(guard), b"q", 0)?;
        let view = view(&scope, control, &outcomes, &absent, &marks, &marks);
        let plan = apply(view, &frame, 0, limits, &mut charge)?;
        if let Some(error) = error {
            assert_generation(&plan, error, &frame)?;
        } else {
            assert!(matches!(
                plan.decision(),
                DispatchDecision::Fresh((Some(_), _))
            ));
        }
    }
    Ok(())
}

fn assert_old_priorities(
    policies: BTreeMap<Vec<u8>, Result<CompletePolicyValue, ApplyError>>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stored = policies[b"r".as_slice()].as_ref().map_err(|_| "stored")?;
    let before = charge.g();
    assert_eq!(
        prepare_complete_policy_replace(Some(stored), 0, Some(0), minimal_data(), limits, charge,)
            .err(),
        Some(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::StaleGeneration)
        ))
    );
    assert_eq!(charge.g(), before + 128);
    let policies = policies
        .into_iter()
        .map(|(key, value)| {
            (
                key,
                value.map(|value| StoredPolicyValue::Complete(Box::new(value))),
            )
        })
        .collect();
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let view = wf_domain::policy::complete::CompleteCorePolicyPatchApplyView {
        gate: GateView {
            scope: &scope,
            control: ControlState::initial(stamp(0)?),
            outcomes: &outcomes,
        },
        revision: 7,
        policies: &policies,
        active_migrations: &marks,
        completed_migrations: &marks,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Ok(100),
    };
    let input = wf_domain::policy::complete::CompleteCorePolicyPatchRequestInput::new(
        scope.clone(),
        RequestId::new(b"q".to_vec())?,
        stamp(0)?,
        wf_domain::policy::complete::CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: Some(0),
            patch: PolicyPatch::default(),
        },
    );
    let frame = wf_ports::log::prepare_complete_core_policy_patch_frame(
        input,
        limits,
        AdmissionLimits::default(),
        charge,
    )?;
    assert_eq!(
        wf_ports::log::decide_complete_core_policy_patch_frame(
            view,
            frame.as_bytes(),
            log(0),
            limits,
            AdmissionLimits::default(),
            charge,
        )
        .err(),
        Some(
            wf_domain::policy::complete::CompleteCorePolicyPatchApplyError::Request(
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::InvalidAuthority
                ))
            )
        )
    );
    Ok(())
}

#[test]
fn complete_replace_parent_retains_only_source_business_origins_and_original_history_limit()
-> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let empty = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let view = view(
        &scope,
        ControlState::initial(stamp(0)?),
        &outcomes,
        &empty,
        &marks,
        &marks,
    );
    let rows = business_rows();
    assert_eq!(rows.len(), 42);
    assert_eq!(
        rows.iter()
            .map(|(_, body)| *body)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        37
    );
    for ((kind, expected), invalid) in rows
        .into_iter()
        .flat_map(|row| [true, false].map(move |invalid| (row, invalid)))
    {
        let supplied = business_data(kind, invalid)?;
        let frame = input_frame(business_data(kind, invalid)?, None)?;
        let plan = apply(view, &frame, 0, limits, &mut charge)?;
        let DispatchDecision::Fresh((write, outcome)) = plan.decision() else {
            return Err("expected reached source content branch".into());
        };
        if invalid {
            assert!(write.is_none());
            let RetainedStatus::CompletePolicyReplaceRejected(reason) = outcome.status() else {
                return Err("caller diagnostic must retain its own source kind".into());
            };
            assert_eq!(reason.kind(), kind);
            assert_eq!(reason.history_max_events_limit(), None);
            assert_outcome(outcome, &frame, expected, (0, 0))?;
        } else {
            let (_, value) = write.as_ref().ok_or("valid neighbor must install")?;
            assert!(value.data() == &supplied);
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
            assert_outcome(outcome, &frame, value.encoded(), (0, 0))?;
        }
        assert!(supplied == business_data(kind, invalid)?);
        assert!(empty.is_empty() && outcomes.is_empty() && marks.is_empty());
    }
    assert_history_limits(view, limits, &mut charge)?;
    assert_content_order(view, limits, &mut charge)?;
    assert_operational_content(view, limits, &mut charge)?;
    Ok(())
}

fn assert_history_limits(
    mut view: CompletePolicyReplaceApplyView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (maximum, expected) in [
        (
            1,
            b"ERR flow retention history_max_events must be between 1 and 1".as_slice(),
        ),
        (
            17,
            b"ERR flow retention history_max_events must be between 1 and 17".as_slice(),
        ),
        (
            1_000_000,
            b"ERR flow retention history_max_events must be between 1 and 1000000".as_slice(),
        ),
    ] {
        view.history_max_events_limit = Ok(maximum);
        for in_state in [false, true] {
            let mut data = minimal_data();
            let retention = RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(maximum + 1),
            };
            if in_state {
                let mut row = state();
                row.retention = Some(retention);
                data.states.insert(b"s".to_vec(), row);
            } else {
                data.retention = Some(retention);
            }
            let frame = input_frame(data, None)?;
            let plan = apply(view, &frame, 0, limits, charge)?;
            let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
                return Err("expected immutable contextual history rejection".into());
            };
            let RetainedStatus::CompletePolicyReplaceRejected(reason) = outcome.status() else {
                return Err("missing content history status".into());
            };
            assert_eq!(reason.kind(), Kind::RetentionHistory { maximum });
            assert_eq!(reason.history_max_events_limit(), Some(maximum));
            assert_outcome(outcome, &frame, expected, (0, 0))?;
        }
    }
    Ok(())
}

fn assert_content_order(
    view: CompletePolicyReplaceApplyView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (first, second) in [
        (
            Kind::GovernanceLimit,
            Kind::GovernancePositive(Field::ApprovalTimeout),
        ),
        (
            Kind::GovernancePositive(Field::FailureThreshold),
            Kind::GovernanceEmptyList(Field::ErrorClasses),
        ),
        (
            Kind::GovernancePercent,
            Kind::GovernancePositive(Field::LatencyMillis),
        ),
        (
            Kind::GovernanceTracking(Tracking::MinCallsAbove64),
            Kind::GovernanceTracking(Tracking::ThresholdAbove64WithoutRate),
        ),
    ] {
        let mut data = business_data(first, true)?;
        let other = business_data(second, true)?;
        let donor = other.governance.ok_or("donor governance")?;
        let gov = data.governance.as_mut().ok_or("priority governance")?;
        if second == Kind::GovernancePositive(Field::ApprovalTimeout) {
            gov.approvals = donor.approvals;
        } else {
            let dst = gov
                .circuits
                .as_mut()
                .ok_or("circuits")?
                .get_mut(b"c".as_slice())
                .ok_or("circuit")?;
            let src = donor
                .circuits
                .ok_or("donor circuits")?
                .remove(b"c".as_slice())
                .ok_or("donor")?;
            match second {
                Kind::GovernanceEmptyList(_) => dst.error_classes = src.error_classes,
                Kind::GovernancePositive(_) => dst.latency_threshold_ms = src.latency_threshold_ms,
                _ => {
                    dst.failure_threshold = src.failure_threshold;
                    dst.failure_rate_pct = src.failure_rate_pct;
                }
            }
        }
        let frame = input_frame(data, None)?;
        let plan = apply(view, &frame, 0, limits, charge)?;
        let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
            return Err("priority branch".into());
        };
        let RetainedStatus::CompletePolicyReplaceRejected(reason) = outcome.status() else {
            return Err("priority content kind".into());
        };
        assert_eq!(reason.kind(), first);
        let body = business_rows()
            .into_iter()
            .find(|(kind, _)| *kind == first)
            .ok_or("body")?
            .1;
        assert_outcome(outcome, &frame, body, (0, 0))?;
    }
    Ok(())
}

fn assert_operational_content(
    mut view: CompletePolicyReplaceApplyView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    view.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    for bytes in [vec![], b" key ".to_vec()] {
        let mut data = minimal_data();
        data.indexed_state_meta = Some(bytes);
        let frame = input_frame(data, None)?;
        assert_eq!(
            apply(view, &frame, 0, limits, charge).err(),
            Some(schema(PolicySchemaError::InvalidIndexedStateMeta))
        );
    }
    let frame = input_frame(business_data(Kind::InvalidVersion, true)?, None)?;
    assert_eq!(
        apply(view, &frame, 0, limits, charge).err(),
        Some(CompletePolicyReplaceApplyError::Apply(
            ApplyError::AllocationFailed
        ))
    );
    Ok(())
}
