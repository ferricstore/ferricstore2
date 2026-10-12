//! Ordered raw parent controls; all authority transfers are mathematical views.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchFresh, CompletePolicyData, CompletePolicyEncodingError,
    CompletePolicyValue, CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, decode_complete_policy_value_v2, prepare_complete_core_policy_patch,
    restore_complete_policy,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError, PolicyLimits};
use wf_domain::retention::RetentionError;
use wf_domain::value::{DomainError, InputField, RetryPolicyField};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{
    DispatchDecision, PolicyRejection, RequestScope, RequestValueError, RetainedOutcome,
    RetainedStatus,
};
use wf_ports::log::{CompleteCorePolicyPatchRequestPlan, decide_complete_core_policy_patch_frame};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::super::{TestResult, minimal_data, present_data, profile};
use super::prepare::cases::{
    core_patch_prepare_command, core_patch_prepare_literal_frame, decode_core_patch_literal,
};
use super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;

mod cases;
mod controls;
use cases::{
    CORE_PATCH_PARENT_BUSINESS, CORE_PATCH_PARENT_MINIMAL, CORE_PATCH_PARENT_VALUES,
    assert_core_patch_parent_accepted, assert_core_patch_parent_control,
    assert_core_patch_parent_current_restoration, assert_core_patch_parent_full_max,
    assert_core_patch_parent_outcome, assert_core_patch_parent_presence, core_patch_parent_apply,
    core_patch_parent_business_frame, core_patch_parent_frame, core_patch_parent_log,
    core_patch_parent_rows, core_patch_parent_scope, core_patch_parent_source8_data,
    core_patch_parent_stamp, core_patch_parent_stored, core_patch_parent_value,
    core_patch_parent_view, core_patch_parent_writer_step,
};
use controls::{
    CORE_PATCH_PARENT_PHASES, assert_core_patch_parent_account, assert_core_patch_parent_limits,
    assert_core_patch_parent_replay, assert_core_patch_parent_writers,
};

#[test]
fn core_patch_parent_fresh_uses_original_partial_and_complete_atomic_bundle() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = core_patch_parent_scope()?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let marks = BTreeMap::new();
    let frame = core_patch_parent_frame(0, b"q", 0)?;
    let view = core_patch_parent_view(&scope, control, &outcomes, &policies, &marks, &marks);
    let first = core_patch_parent_apply(view, &frame, 0, limits, &mut charge)?;
    assert_core_patch_parent_accepted(
        &first,
        &minimal_data(),
        CORE_PATCH_PARENT_MINIMAL,
        0,
        &frame,
        (b"q", 0, 0),
    )?;
    assert_core_patch_parent_control(&first, view, &frame, 0, 0, false)?;
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (11_178, 0, 11_178)
    );
    let (revision, base, proposed, decision) = first.into_parts();
    assert_eq!((revision, base), (7, control));
    assert_eq!(proposed.next().position().get(), 0);
    let DispatchDecision::Fresh((Some((0, first_value)), first_outcome)) = decision else {
        return Err("missing whole first proposed bundle".into());
    };
    assert_eq!(first_value.encoded(), CORE_PATCH_PARENT_MINIMAL);
    assert_eq!(first_outcome.response(), CORE_PATCH_PARENT_MINIMAL);
    assert!(policies.is_empty() && outcomes.is_empty() && marks.is_empty());

    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(core_patch_parent_value(0, limits, &mut charge)?),
    )]);
    let frame = core_patch_parent_frame(7, b"q", 0)?;
    let data = core_patch_parent_source8_data();
    for (active_generation, completed_generation, generation) in [(0, 0, 2_u64), (5, 7, 8)] {
        let active = BTreeMap::from([(b"r".to_vec(), Ok(active_generation))]);
        let done = BTreeMap::from([(b"r".to_vec(), Ok(completed_generation))]);
        let mut expected = decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[3].0)?;
        expected[6..14].copy_from_slice(&generation.to_be_bytes());
        let view = core_patch_parent_view(&scope, control, &outcomes, &policies, &active, &done);
        let plan = core_patch_parent_apply(view, &frame, 0, limits, &mut charge)?;
        assert_core_patch_parent_accepted(&plan, &data, &expected, 1, &frame, (b"q", 0, 0))?;
        assert_core_patch_parent_control(&plan, view, &frame, 0, 0, false)?;
        assert_eq!(active.get(b"r".as_slice()), Some(&Ok(active_generation)));
        assert_eq!(done.get(b"r".as_slice()), Some(&Ok(completed_generation)));
        assert_eq!(
            core_patch_parent_stored(&policies)?.encoded(),
            decode_core_patch_literal(CORE_PATCH_PARENT_VALUES[0].0)?.as_slice()
        );
        assert!(outcomes.is_empty());
    }
    assert_core_patch_parent_presence(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_parent_replays_before_reconstruction_current_and_context() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_parent_replay(limits, &mut charge, false)?;
    assert_core_patch_parent_replay(limits, &mut charge, true)?;
    Ok(())
}

#[test]
fn core_patch_parent_preserves_stored_cas_context_and_high_water_order() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_parent_current_order(limits, &mut charge)?;
    assert_core_patch_parent_full_max(limits, &mut charge)?;
    let scope = core_patch_parent_scope()?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let frame = core_patch_parent_frame(0, b"q", 0)?;
    for case in 0..7 {
        let active = BTreeMap::from([(
            b"r".to_vec(),
            match case {
                1 => Err(ApplyError::PositionConflict),
                2 | 3 => Ok(MAX_POLICY_GENERATION),
                4 => Ok(MAX_POLICY_GENERATION + 1),
                _ => Ok(2),
            },
        )]);
        let done = BTreeMap::from([(
            b"r".to_vec(),
            match case {
                1 | 2 | 4 => Err(ApplyError::AllocationFailed),
                5 => Ok(MAX_POLICY_GENERATION - 1),
                _ => Ok(5),
            },
        )]);
        let mut view =
            core_patch_parent_view(&scope, control, &outcomes, &policies, &active, &done);
        if case == 0 {
            view.history_max_events_limit = Err(ApplyError::AllocationFailed);
        }
        let before = charge.g();
        let result = core_patch_parent_apply(view, &frame, 0, limits, &mut charge);
        if case <= 2 || case == 4 {
            let (error, fee) = match case {
                0 => (ApplyError::AllocationFailed, 6_788),
                1 => (ApplyError::PositionConflict, 8_455),
                2 => (ApplyError::AllocationFailed, 8_584),
                _ => (ApplyError::InvalidAuthority, 8_455),
            };
            assert_eq!(
                result.err().ok_or("expected ordered authority refusal")?,
                CompleteCorePolicyPatchApplyError::Apply(error)
            );
            assert_eq!((charge.g(), charge.p()), (before + fee, 128));
        } else {
            let plan = result?;
            assert_core_patch_parent_control(&plan, view, &frame, 0, 0, false)?;
            if case == 3 {
                let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
                    return Err("expected late exhaustion business outcome".into());
                };
                assert_core_patch_parent_outcome(
                    outcome,
                    CORE_PATCH_PARENT_BUSINESS[6].1,
                    RetainedStatus::PolicyRejected(PolicyRejection::new(
                        PolicyError::GenerationExhausted,
                    )?),
                    &frame[83..],
                    (b"q", 0, 0),
                )?;
            } else {
                let mut expected = CORE_PATCH_PARENT_MINIMAL.to_vec();
                expected[6..14].copy_from_slice(
                    &(if case == 5 { MAX_POLICY_GENERATION } else { 6 }).to_be_bytes(),
                );
                assert_core_patch_parent_accepted(
                    &plan,
                    &minimal_data(),
                    &expected,
                    0,
                    &frame,
                    (b"q", 0, 0),
                )?;
            }
        }
        assert!(policies.is_empty() && outcomes.is_empty());
        assert_eq!(active.len(), 1);
        assert_eq!(done.len(), 1);
    }
    Ok(())
}

#[test]
fn core_patch_parent_classifies_business_and_operational_failures() -> TestResult {
    let limits = profile(30_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = core_patch_parent_scope()?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let done = BTreeMap::new();
    for (case, &(error, body)) in CORE_PATCH_PARENT_BUSINESS.iter().enumerate() {
        let frame = core_patch_parent_business_frame(case)?;
        let active = if case == 6 {
            BTreeMap::from([(b"r".to_vec(), Ok(MAX_POLICY_GENERATION))])
        } else {
            BTreeMap::new()
        };
        let view = core_patch_parent_view(&scope, control, &outcomes, &policies, &active, &done);
        let plan = core_patch_parent_apply(view, &frame, 0, limits, &mut charge)?;
        assert_core_patch_parent_control(&plan, view, &frame, 0, 0, false)?;
        let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
            return Err("expected exact business-only retained body".into());
        };
        assert_core_patch_parent_outcome(
            outcome,
            body,
            RetainedStatus::PolicyRejected(PolicyRejection::new(error)?),
            &frame[83..],
            (b"q", 0, 0),
        )?;
        assert!(policies.is_empty() && outcomes.is_empty() && done.is_empty());
    }
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for length in [262_144, 262_145] {
        let mut intent = minimal[83..].to_vec();
        let mut payload = vec![1, 0];
        payload.extend_from_slice(&u32::try_from(length)?.to_be_bytes());
        payload.extend(std::iter::repeat_n(b'x', length));
        drop(intent.splice(44..45, payload));
        let frame = core_patch_prepare_literal_frame(
            &intent,
            [b"t", b"g", b"r", b"q"],
            core_patch_parent_stamp(0)?,
        )?;
        let mut view = core_patch_parent_view(&scope, control, &outcomes, &policies, &done, &done);
        view.outcome_retention_ms = Err(ApplyError::AllocationFailed);
        let expected = if length == 262_144 {
            CompletePolicyEncodingError::Schema(PolicySchemaError::SnapshotBytes)
        } else {
            CompletePolicyEncodingError::ByteLimit
        };
        assert_eq!(
            core_patch_parent_apply(view, &frame, 0, limits, &mut charge)
                .err()
                .ok_or("raw/merged caps remain operational before window")?,
            CompleteCorePolicyPatchApplyError::Request(expected.into())
        );
    }
    let before = charge.g();
    let frame = core_patch_parent_frame(0, b"q", 0)?;
    let view = core_patch_parent_view(&scope, control, &outcomes, &policies, &done, &done);
    assert_eq!(
        decide_complete_core_policy_patch_frame(
            view,
            &frame,
            core_patch_parent_log(0),
            profile(1)?,
            AdmissionLimits::default(),
            &mut charge
        )
        .err()
        .ok_or("entry profile refusal")?,
        CompleteCorePolicyPatchApplyError::Request(PolicySchemaError::ProfileMismatch.into())
    );
    assert_eq!(charge.g(), before);
    assert_core_patch_parent_operational(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_parent_threads_one_account_through_every_reached_phase() -> TestResult {
    for &(target, retained) in CORE_PATCH_PARENT_PHASES {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let scope = core_patch_parent_scope()?;
        let empty = BTreeMap::new();
        let migrations = BTreeMap::new();
        let outcomes = BTreeMap::new();
        let frame = core_patch_parent_frame(0, b"q", 0)?;
        let view = core_patch_parent_view(
            &scope,
            ControlState::initial(core_patch_parent_stamp(0)?),
            &outcomes,
            &empty,
            &migrations,
            &migrations,
        );
        let result = core_patch_parent_apply(view, &frame, 0, limits, &mut charge);
        assert_eq!(
            result
                .err()
                .ok_or("expected prospective same-account refusal")?,
            CompleteCorePolicyPatchApplyError::Request(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum
                }
                .into()
            )
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained, 0, retained)
        );
        assert!(empty.is_empty() && outcomes.is_empty() && migrations.is_empty());
        assert_eq!(view.gate.control.last_entry(), None);
    }
    for (target, attempts, peak) in [
        (11_178, 1, false),
        (22_356, 2, false),
        (13_170, 1, true),
        (24_348, 2, true),
    ] {
        assert_core_patch_parent_account(target, target, attempts, peak)?;
        assert_core_patch_parent_account(target - 1, target, attempts, peak)?;
    }
    assert_core_patch_parent_current_restoration()?;
    Ok(())
}

#[test]
fn core_patch_parent_serializes_independent_writers_without_renewing_outcomes() -> TestResult {
    let limits = profile(50_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_parent_writers(limits, &mut charge)?;
    Ok(())
}

fn assert_core_patch_parent_current_order(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let outcomes = BTreeMap::new();
    let migrations = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let frame = core_patch_parent_frame(1, b"q", 0)?;
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let mut policies = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let mut view = core_patch_parent_view(
        &scope,
        control,
        &outcomes,
        &policies,
        &migrations,
        &migrations,
    );
    view.history_max_events_limit = Err(ApplyError::AllocationFailed);
    view.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    assert_eq!(
        core_patch_parent_apply(view, &frame, 0, limits, charge)
            .err()
            .ok_or("stored read must precede stale CAS")?,
        CompleteCorePolicyPatchApplyError::Apply(ApplyError::AllocationFailed)
    );
    let mut foreign = CORE_PATCH_PARENT_MINIMAL.to_vec();
    foreign[28] = b'u';
    let value = decode_complete_policy_value_v2(&foreign, limits, charge)?;
    policies.insert(b"r".to_vec(), Ok(value));
    for case in 0..2 {
        if case == 1 {
            let candidate = restore_complete_policy(1, minimal_data(), limits, charge)?;
            let mut malformed = CORE_PATCH_PARENT_MINIMAL.to_vec();
            malformed[0] ^= 1;
            let value = candidate.bind_encoded_snapshot(18..40, malformed, limits, charge)?;
            policies.insert(b"r".to_vec(), Ok(value));
        }
        let mut view = core_patch_parent_view(
            &scope,
            control,
            &outcomes,
            &policies,
            &migrations,
            &migrations,
        );
        view.history_max_events_limit = Err(ApplyError::AllocationFailed);
        view.outcome_retention_ms = Err(ApplyError::AllocationFailed);
        let result = core_patch_parent_apply(view, &frame, 0, limits, charge);
        let expected = if case == 0 {
            CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority)
        } else {
            CompletePolicyEncodingError::EncodingMismatch
        };
        assert_eq!(
            result
                .err()
                .ok_or("current authority must precede stale CAS")?,
            CompleteCorePolicyPatchApplyError::Request(expected.into())
        );
        let current = core_patch_parent_stored(&policies)?;
        let before = charge.g();
        assert_eq!(
            prepare_complete_core_policy_patch(
                Some(current),
                MAX_POLICY_GENERATION,
                core_patch_prepare_command(1)?,
                0,
                limits,
                charge
            )
            .err()
            .ok_or("standalone remains guard first")?,
            CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
                PolicyError::StaleGeneration
            ))
        );
        assert_eq!(charge.g(), before + 128);
        assert_eq!(current.generation(), 1);
        assert!(outcomes.is_empty());
    }
    Ok(())
}

fn assert_core_patch_parent_operational(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = core_patch_parent_scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let migrations = BTreeMap::new();
    let control = ControlState::initial(core_patch_parent_stamp(0)?);
    let default = core_patch_parent_frame(0, b"q", 0)?;
    for case in 0..6 {
        let mut view =
            core_patch_parent_view(&scope, control, &outcomes, &empty, &migrations, &migrations);
        let (frame, admission, expected) = match case {
            0 => {
                view.outcome_retention_ms = Err(ApplyError::AllocationFailed);
                (
                    default.clone(),
                    AdmissionLimits::default(),
                    CompleteCorePolicyPatchApplyError::Apply(ApplyError::AllocationFailed),
                )
            }
            1 => {
                view.outcome_retention_ms = Ok(0);
                (
                    core_patch_parent_business_frame(5)?,
                    AdmissionLimits::default(),
                    CompleteCorePolicyPatchApplyError::Outcome(RequestValueError::InvalidExpiry),
                )
            }
            2 => {
                view.history_max_events_limit = Ok(0);
                (
                    default.clone(),
                    AdmissionLimits::default(),
                    CompleteCorePolicyPatchApplyError::Apply(ApplyError::InvalidAuthority),
                )
            }
            3 => (
                default.clone(),
                AdmissionLimits::new(1_048_576, 134, 1_000)?,
                CompleteCorePolicyPatchApplyError::Request(
                    CompletePolicyEncodingError::ByteLimit.into(),
                ),
            ),
            4 => {
                view.outcome_retention_ms = Err(ApplyError::PositionConflict);
                (
                    core_patch_parent_business_frame(5)?,
                    AdmissionLimits::default(),
                    CompleteCorePolicyPatchApplyError::Apply(ApplyError::PositionConflict),
                )
            }
            _ => {
                view.outcome_retention_ms = Ok(u64::MAX);
                let stale = core_patch_parent_business_frame(5)?;
                let frame = core_patch_prepare_literal_frame(
                    &stale[83..],
                    [b"t", b"g", b"r", b"q"],
                    core_patch_parent_stamp(1)?,
                )?;
                (
                    frame,
                    AdmissionLimits::default(),
                    CompleteCorePolicyPatchApplyError::Outcome(RequestValueError::InvalidExpiry),
                )
            }
        };
        let before = charge.g();
        assert_eq!(
            decide_complete_core_policy_patch_frame(
                view,
                &frame,
                core_patch_parent_log(0),
                limits,
                admission,
                charge
            )
            .err()
            .ok_or("operational failure returns no parent")?,
            expected
        );
        if case == 0 {
            assert_eq!(charge.g(), before + 10_626);
        }
        if case == 2 {
            assert_eq!(charge.g(), before + 6_788);
        }
        if case == 3 {
            assert_eq!(charge.g(), before);
        }
        assert!(outcomes.is_empty() && empty.is_empty() && migrations.is_empty());
    }
    assert_core_patch_parent_limits(limits, charge)?;
    Ok(())
}

fn assert_core_patch_parent_replay_branch(
    view: CompleteCorePolicyPatchApplyView<'_>,
    incoming: &[u8],
    original: (&[u8], &[u8], RetainedStatus),
    scenario: (usize, u64, u64),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let (branch, time, verifier) = scenario;
    let index = u64::from(branch != 3);
    let before = charge.g();
    let plan = core_patch_parent_apply(view, incoming, index, limits, charge)?;
    assert_core_patch_parent_control(&plan, view, incoming, index, time, branch == 3)?;
    let retained = view
        .gate
        .outcomes
        .get(&RequestId::new(b"q".to_vec())?)
        .ok_or("held original parent-derived outcome")?;
    assert_core_patch_parent_outcome(
        retained,
        original.1,
        original.2,
        &original.0[83..],
        (b"q", 10, 0),
    )?;
    let fee = match branch {
        1 => 1_107,
        3 => 390,
        _ => 1_235,
    };
    assert_eq!((charge.g(), charge.p()), (before + verifier + fee, 0));
    assert_eq!(view.gate.outcomes.len(), 1);
    assert_eq!(
        view.policies
            .get(b"r".as_slice())
            .ok_or("poison current")?
            .as_ref()
            .err(),
        Some(&ApplyError::AllocationFailed)
    );
    assert_eq!(
        view.active_migrations.get(b"r".as_slice()),
        Some(&Err(ApplyError::PositionConflict))
    );
    assert_eq!(
        view.completed_migrations.get(b"r".as_slice()),
        Some(&Err(ApplyError::PositionConflict))
    );
    assert_eq!(
        view.history_max_events_limit,
        Err(ApplyError::AllocationFailed)
    );
    assert_eq!(view.outcome_retention_ms, Err(ApplyError::PositionConflict));
    match plan.decision() {
        DispatchDecision::Replay(replayed) if branch == 0 => {
            assert_core_patch_parent_outcome(
                replayed,
                original.1,
                original.2,
                &original.0[83..],
                (b"q", 10, 0),
            )?;
            assert!(std::ptr::eq(*replayed, retained));
            assert_eq!(replayed.response().as_ptr(), retained.response().as_ptr());
        }
        DispatchDecision::IntentConflict if branch == 1 => {}
        DispatchDecision::Expired if branch == 2 => {}
        DispatchDecision::LogReplay if branch == 3 => {}
        _ => return Err("nonFresh branch must not reach reconstruction".into()),
    }
    Ok(())
}
