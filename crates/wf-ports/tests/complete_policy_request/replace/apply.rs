//! Closed raw Replace parent: supplied pure views do not implement storage.

use sha2::Digest;
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError, CompletePolicyReplaceApplyError,
    CompletePolicyReplaceApplyView, CompletePolicyReplaceFresh, CompletePolicyRequestError,
    CompletePolicyValue, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::{
    DispatchDecision, RequestScope, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{CompletePolicyReplaceRequestPlan, decide_complete_policy_replace_frame};
use wf_types::identity::RequestId;

use super::super::{TestResult, minimal_data, present_data, profile};

mod cases;
mod controls;
mod literals;

use cases::{current, input_frame, literal_frame, log, schema, scope, stamp, view};
use literals::{assert_accepted, assert_control, assert_outcome, value_bytes};

fn apply<'a>(
    view: CompletePolicyReplaceApplyView<'a>,
    frame: &[u8],
    index: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceRequestPlan<'a>, CompletePolicyReplaceApplyError> {
    decide_complete_policy_replace_frame(
        view,
        frame,
        log(index),
        limits,
        AdmissionLimits::default(),
        charge,
    )
}

#[test]
fn complete_replace_parent_installs_frozen_original_bundle_and_resets_omissions() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let control = ControlState::initial(stamp(0)?);
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let marks = BTreeMap::new();
    let frame = literal_frame(false, None, b"q", 0)?;
    let expected = value_bytes(1, false)?;
    let first_view = view(&scope, control, &outcomes, &policies, &marks, &marks);
    let plan = apply(first_view, &frame, 0, limits, &mut charge)?;
    assert_accepted(&plan, &minimal_data(), &expected, 0, &frame, (0, 0))?;
    assert_control(&plan, first_view, &frame, (0, 0), false)?;
    assert_eq!((charge.g(), charge.p(), charge.total()), (9_640, 0, 9_640));
    assert!(policies.is_empty() && outcomes.is_empty() && marks.is_empty());

    let full_frame = literal_frame(true, None, b"q", 0)?;
    let expected_full = value_bytes(1, true)?;
    let full_plan = apply(first_view, &full_frame, 0, limits, &mut charge)?;
    assert_accepted(
        &full_plan,
        &present_data(),
        &expected_full,
        0,
        &full_frame,
        (0, 0),
    )?;
    assert_control(&full_plan, first_view, &full_frame, (0, 0), false)?;
    let (revision, base, proposed, decision) = full_plan.into_parts();
    assert_eq!((revision, base), (7, control));
    let DispatchDecision::Fresh((Some((0, value)), outcome)) = decision else {
        return Err("whole replacement bundle is missing".into());
    };
    let policies = BTreeMap::from([(b"r".to_vec(), Ok(value))]);
    let outcomes = BTreeMap::from([(RequestId::new(b"q".to_vec())?, outcome)]);
    let frame = literal_frame(false, Some(1), b"a", 1)?;
    let expected = value_bytes(2, false)?;
    let next_view = view(
        &scope,
        proposed.next(),
        &outcomes,
        &policies,
        &marks,
        &marks,
    );
    let plan = apply(next_view, &frame, 1, limits, &mut charge)?;
    assert_accepted(&plan, &minimal_data(), &expected, 1, &frame, (1, 1))?;
    assert_control(&plan, next_view, &frame, (1, 1), false)?;
    let stored = policies[b"r".as_slice()].as_ref().map_err(|_| "stored")?;
    assert!(stored.data() == &present_data());
    assert_eq!(stored.encoded(), expected_full);
    let DispatchDecision::Fresh((Some((_, new_value)), _)) = plan.decision() else {
        return Err("missing omitted replacement".into());
    };
    assert_eq!(new_value.data().max_active_ms, None);
    assert!(new_value.data().states.is_empty());
    assert!(new_value.data().governance.is_none());
    assert_eq!(new_value.data().indexed_attributes, None);
    Ok(())
}

fn origin(
    rejected: bool,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<(CompletePolicyReplaceFresh, ControlState, Vec<u8>, Vec<u8>)> {
    let scope = scope()?;
    let control = ControlState::initial(stamp(0)?);
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let marks = BTreeMap::new();
    let frame = if rejected {
        let mut data = minimal_data();
        data.retention = Some(wf_domain::retention::RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(18),
        });
        input_frame(data, None)?
    } else {
        literal_frame(false, None, b"q", 0)?
    };
    let expected = if rejected {
        b"ERR flow retention history_max_events must be between 1 and 17".to_vec()
    } else {
        value_bytes(1, false)?
    };
    let mut fresh = view(&scope, control, &outcomes, &policies, &marks, &marks);
    fresh.history_max_events_limit = Ok(17);
    let before = charge.g();
    let plan = apply(fresh, &frame, 0, limits, charge)?;
    assert_eq!(charge.g(), before + if rejected { 9_182 } else { 9_640 });
    assert_control(&plan, fresh, &frame, (0, 0), false)?;
    let next = plan.control().next();
    let (_, _, _, decision) = plan.into_parts();
    let DispatchDecision::Fresh(bundle) = decision else {
        return Err("real origin".into());
    };
    assert_outcome(&bundle.1, &frame, &expected, (0, 0))?;
    if rejected {
        assert!(bundle.0.is_none());
        let RetainedStatus::CompletePolicyReplaceRejected(reason) = bundle.1.status() else {
            return Err("real reached CONTENT1 origin".into());
        };
        assert_eq!(
            reason.kind(),
            wf_domain::policy::complete::CompletePolicyReplaceRejectionKind::RetentionHistory {
                maximum: 17
            }
        );
        assert_eq!(reason.history_max_events_limit(), Some(17));
    } else {
        assert_eq!(bundle.1.status(), RetainedStatus::Accepted);
        let (_, value) = bundle.0.as_ref().ok_or("accepted origin value")?;
        assert!(value.data() == &minimal_data());
        assert_eq!(value.encoded(), expected);
    }
    Ok((bundle, next, frame, expected))
}

#[test]
fn complete_replace_parent_replays_real_accepted_and_content_rejected_outcomes_before_fresh_work()
-> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    for rejected in [false, true] {
        let (bundle, control, frame, expected) = origin(rejected, limits, &mut charge)?;
        let request = RequestId::new(b"q".to_vec())?;
        let outcomes = BTreeMap::from([(request.clone(), bundle.1)]);
        let policies = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
        let active = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::InvalidAuthority))]);
        let done = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
        let mut replay_frame = frame.clone();
        replay_frame[61..69].copy_from_slice(&50_u64.to_be_bytes());
        replay_frame[71..79].copy_from_slice(&50_u64.to_be_bytes());
        let mut replay_view = view(&scope, control, &outcomes, &policies, &active, &done);
        replay_view.history_max_events_limit = Err(ApplyError::AllocationFailed);
        replay_view.outcome_retention_ms = Err(ApplyError::InvalidAuthority);
        let before = charge.g();
        let plan = apply(replay_view, &replay_frame, 1, limits, &mut charge)?;
        let DispatchDecision::Replay(replayed) = plan.decision() else {
            return Err("changed Fresh authorities must not mask actual replay".into());
        };
        assert_control(&plan, replay_view, &replay_frame, (50, 1), false)?;
        assert_outcome(replayed, &frame, &expected, (0, 0))?;
        assert_eq!(replayed.status(), outcomes[&request].status());
        if rejected {
            let RetainedStatus::CompletePolicyReplaceRejected(reason) = replayed.status() else {
                return Err("history replay status".into());
            };
            assert_eq!(reason.history_max_events_limit(), Some(17));
        }
        assert_eq!(charge.g(), before + if rejected { 6_060 } else { 5_658 });
        assert_eq!(
            replayed.response().as_ptr(),
            outcomes[&request].response().as_ptr()
        );
        assert!(std::ptr::eq::<RetainedOutcome>(
            *replayed,
            &raw const outcomes[&request]
        ));
        if let Some((0, value)) = &bundle.0 {
            assert_eq!(value.encoded(), value_bytes(1, false)?);
        }
    }
    Ok(())
}

#[test]
fn complete_replace_parent_keeps_nonfresh_gate_priority_and_binding() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for rejected in [false, true] {
        let (bundle, control, original, expected) = origin(rejected, limits, &mut charge)?;
        let scope = scope()?;
        let request = RequestId::new(b"q".to_vec())?;
        let outcomes = BTreeMap::from([(request.clone(), bundle.1)]);
        let policies = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
        let marks = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
        let mut base = view(&scope, control, &outcomes, &policies, &marks, &marks);
        base.history_max_events_limit = Err(ApplyError::PositionConflict);
        base.outcome_retention_ms = Err(ApplyError::InvalidAuthority);
        for (case, time, index, fee) in [
            (0, 0_u64, 0, 4_813),
            (1, 100, 1, 5_658),
            (2, 99, 1, 5_658),
            (3, 100, 1, 5_530),
        ] {
            let mut frame = original.clone();
            frame[61..69].copy_from_slice(&time.to_be_bytes());
            frame[71..79].copy_from_slice(&time.to_be_bytes());
            if case == 3 {
                // A structurally valid different type at the same width.
                frame[123] ^= 1;
                let digest: [u8; 32] = sha2::Sha256::digest(&frame[83..]).into();
                frame[29..61].copy_from_slice(&digest);
            }
            let before = charge.g();
            let plan = apply(base, &frame, index, limits, &mut charge)?;
            assert_control(&plan, base, &frame, (time, index), case == 0)?;
            match case {
                0 => assert!(matches!(plan.decision(), DispatchDecision::LogReplay)),
                1 => assert!(matches!(plan.decision(), DispatchDecision::Expired)),
                2 => assert!(matches!(plan.decision(), DispatchDecision::Replay(_))),
                _ => assert!(matches!(plan.decision(), DispatchDecision::IntentConflict)),
            }
            assert_eq!(charge.g(), before + fee + if rejected { 402 } else { 0 });
            assert_outcome(&outcomes[&request], &original, &expected, (0, 0))?;
        }
        let mut bad = original.clone();
        bad[6] = 5;
        assert_eq!(
            apply(base, &bad, 1, limits, &mut charge).err(),
            Some(CompletePolicyReplaceApplyError::Request(
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::UnsupportedTag)
            ))
        );
        let business_invalid = input_frame(
            cases::business_data(
                wf_domain::policy::complete::CompletePolicyReplaceRejectionKind::InvalidVersion,
                true,
            )?,
            None,
        )?;
        let conflict = apply(base, &business_invalid, 1, limits, &mut charge)?;
        assert!(matches!(
            conflict.decision(),
            DispatchDecision::IntentConflict
        ));
        bad = original.clone();
        bad[29] ^= 1;
        assert_eq!(
            apply(base, &bad, 1, limits, &mut charge).err(),
            Some(CompletePolicyReplaceApplyError::Request(
                CompletePolicyRequestError::IntentMismatch
            ))
        );
        let foreign_scope = RequestScope::new(
            wf_types::identity::TenantId::new(b"foreign".to_vec())?,
            wf_types::identity::GroupId::new(b"g".to_vec())?,
            wf_types::identity::RecoveryId::new(b"r".to_vec())?,
        );
        let mut foreign = base;
        foreign.gate.scope = &foreign_scope;
        assert_eq!(
            apply(foreign, &original, 1, limits, &mut charge).err(),
            Some(CompletePolicyReplaceApplyError::Apply(
                ApplyError::InvalidAuthority
            ))
        );
    }
    Ok(())
}

#[test]
fn complete_replace_parent_sequential_replacements_preserve_generations_and_stale_results()
-> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let mut control = ControlState::initial(stamp(0)?);
    let mut policies = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    for writer in 0_u8..12 {
        let generation = u64::from(writer) + 1;
        let version = u64::from(writer) * 1_000;
        let request = [b'a' + writer];
        let time = u64::from(writer);
        let mut data = minimal_data();
        data.version = Some(wf_domain::policy::definition::PolicyVersion::Number(
            version,
        ));
        let mut intent = super::super::MINIMAL_INTENT_BYTES[..41].to_vec();
        intent.extend_from_slice(&[1, 1]);
        intent.extend_from_slice(&version.to_be_bytes());
        intent.extend_from_slice(&super::super::MINIMAL_INTENT_BYTES[42..]);
        let mut expected = b"FWPV\0\x02".to_vec();
        expected.extend_from_slice(&generation.to_be_bytes());
        expected.extend_from_slice(&31_u32.to_be_bytes());
        expected.extend_from_slice(b"FWPS\0\x02");
        expected.extend_from_slice(&intent[36..]);
        let frame = super::literals::replace_frame_literal(
            &intent,
            [b"t", b"g", b"r", &request],
            stamp(time)?,
        )?;
        let next_view = view(&scope, control, &outcomes, &policies, &marks, &marks);
        let plan = apply(next_view, &frame, time, limits, &mut charge)?;
        assert_accepted(
            &plan,
            &data,
            &expected,
            generation - 1,
            &frame,
            (time, time),
        )?;
        assert_control(&plan, next_view, &frame, (time, time), false)?;
        let (_, _, proposed, decision) = plan.into_parts();
        let DispatchDecision::Fresh((Some((_, value)), outcome)) = decision else {
            return Err("sequential replacement writer".into());
        };
        control = proposed.next();
        policies.insert(b"r".to_vec(), Ok(value));
        outcomes.insert(RequestId::new(request.to_vec())?, outcome);
        assert_eq!(outcomes.len(), usize::from(writer) + 1);
        assert!(
            policies[b"r".as_slice()]
                .as_ref()
                .map_err(|_| "stored")?
                .data()
                == &data
        );
        assert_eq!(
            policies[b"r".as_slice()]
                .as_ref()
                .map_err(|_| "stored")?
                .encoded(),
            expected
        );
    }
    assert_writer_stale(
        &scope,
        control,
        &policies,
        &mut outcomes,
        limits,
        &mut charge,
    )
}

fn assert_writer_stale(
    scope: &RequestScope,
    control: ControlState,
    policies: &BTreeMap<Vec<u8>, Result<CompletePolicyValue, ApplyError>>,
    outcomes: &mut BTreeMap<RequestId, RetainedOutcome>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let marks = BTreeMap::new();
    let stale_frame = literal_frame(false, Some(1), b"q", 12)?;
    let stale_view = view(scope, control, outcomes, policies, &marks, &marks);
    let plan = apply(stale_view, &stale_frame, 12, limits, charge)?;
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("stale sequential request cannot write".into());
    };
    assert_eq!(
        outcome.response(),
        literals::generation_body(PolicyError::StaleGeneration)?
    );
    assert_outcome(
        outcome,
        &stale_frame,
        literals::generation_body(PolicyError::StaleGeneration)?,
        (12, 12),
    )?;
    let (_, _, proposed, decision) = plan.into_parts();
    let DispatchDecision::Fresh((None, outcome)) = decision else {
        return Err("stale outcome".into());
    };
    outcomes.insert(RequestId::new(b"q".to_vec())?, outcome);
    let before_value = policies[b"r".as_slice()]
        .as_ref()
        .map_err(|_| "stored")?
        .encoded()
        .to_vec();
    let mut replay_view = view(scope, proposed.next(), outcomes, policies, &marks, &marks);
    replay_view.history_max_events_limit = Err(ApplyError::AllocationFailed);
    replay_view.outcome_retention_ms = Err(ApplyError::PositionConflict);
    let mut replay_frame = stale_frame.clone();
    replay_frame[61..69].copy_from_slice(&13_u64.to_be_bytes());
    replay_frame[71..79].copy_from_slice(&13_u64.to_be_bytes());
    let plan = apply(replay_view, &replay_frame, 13, limits, charge)?;
    assert!(matches!(plan.decision(), DispatchDecision::Replay(_)));
    assert_eq!(
        policies[b"r".as_slice()]
            .as_ref()
            .map_err(|_| "stored")?
            .encoded(),
        before_value
    );
    assert_eq!(
        policies[b"r".as_slice()]
            .as_ref()
            .map_err(|_| "stored")?
            .generation(),
        12
    );
    Ok(())
}
