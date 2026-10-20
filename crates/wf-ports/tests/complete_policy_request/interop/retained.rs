//! Retained decisions skip changed current formats and preserve original replies.

use super::{COMPLETE, TestResult, frame, log, metadata, original, profile, stamp};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyView, OriginalCompletePolicyReplaceApplyView,
    OriginalPolicyReadValue, OriginalPolicySetApplyView, OriginalPolicySetFrameInput,
    OriginalPolicySetMapOrders, PolicySchemaCharge, RawFlowPolicyValue as Raw,
    canonical_original_etf_safety, decode_complete_policy_value_v2,
    prepare_original_policy_set_request,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_domain::workflow::request::gate::{GateView, PointGateView};
use wf_domain::workflow::request::{DispatchDecision, RetainedOutcome, RetainedStatus};
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, OriginalPolicySetPointInspection,
    decide_complete_core_policy_patch_frame, decide_original_policy_set_frame,
    inspect_complete_core_policy_patch_point, inspect_original_policy_set_point_frame_bounded,
    prepare_complete_core_policy_patch_point_frame, prepare_original_policy_set_frame,
    prepare_original_policy_set_point_frame_bounded, verify_complete_core_policy_patch_frame,
};
use wf_ports::state::{GroupCorePolicyChanges, GroupOriginalPolicyChanges, StoredPolicyValue};
use wf_types::version::TimestampMillis;

const PATCH: &[u8] = &[0, 0, 0, 17, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17];

fn classification<T>(
    decision: &DispatchDecision<'_, T>,
    class: usize,
    original: &RetainedOutcome,
) -> TestResult {
    match (class, decision) {
        (0, DispatchDecision::LogReplay)
        | (2, DispatchDecision::IntentConflict)
        | (3, DispatchDecision::Expired) => {}
        (1, DispatchDecision::Replay(outcome)) => {
            assert!(std::ptr::eq(*outcome, original));
            assert_eq!(outcome.response().as_ptr(), original.response().as_ptr());
            assert_eq!(*outcome, original);
            assert_eq!(
                (
                    outcome.original_position(),
                    outcome.original_time().get(),
                    outcome.expires_at().get()
                ),
                (log(0), 10, 110)
            );
        }
        _ => return Err("retained decision entered Fresh or changed classification".into()),
    }
    Ok(())
}

fn retained_core(version: u16, encoded: &[u8]) -> TestResult<RetainedOutcome> {
    let schema = profile(100_000_000)?;
    let mut wanted = metadata(RetainedStatus::Accepted)?;
    wanted.intent = verify_complete_core_policy_patch_frame(
        encoded,
        schema,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(schema),
    )?
    .header(log(0))
    .intent;
    wanted.original_position = log(0);
    wanted.original_time = TimestampMillis::new(10);
    wanted.expires_at = TimestampMillis::new(110);
    Ok(RetainedOutcome::new(
        wanted,
        version,
        if version == 1 {
            COMPLETE.to_vec()
        } else {
            original(1)?
        },
        AdmissionLimits::default(),
    )?)
}

fn core(version: u16) -> TestResult {
    let schema = profile(100_000_000)?;
    let scope = metadata(RetainedStatus::Accepted)?.scope;
    let encoded = frame(10, None, PolicyPatch::default())?;
    let original = retained_core(version, &encoded)?;
    let verified = verify_complete_core_policy_patch_frame(
        &encoded,
        schema,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(schema),
    )?;
    let control = ControlState::restored(
        stamp(10)?,
        verified.header(log(0)).identity,
        EntryStamp::Stamped(stamp(10)?),
    )?;
    let mut outcomes = BTreeMap::new();
    outcomes.insert(original.request().clone(), original);
    let original = outcomes.values().next().ok_or("retained original")?;
    let current = if version == 1 {
        StoredPolicyValue::Original(super::original(1)?)
    } else {
        StoredPolicyValue::Complete(Box::new(decode_complete_policy_value_v2(
            COMPLETE,
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?))
    };
    let policies = BTreeMap::from([(b"r".to_vec(), Ok(current))]);
    let migrations = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::InvalidAuthority))]);
    for class in 0..4 {
        let patch = PolicyPatch {
            max_active_ms: if class == 2 {
                FieldPatch::Set(5)
            } else {
                FieldPatch::Unchanged
            },
            ..PolicyPatch::default()
        };
        let body = frame(if class == 3 { 110 } else { 10 }, None, patch)?;
        let mut charge = PolicySchemaCharge::new(schema);
        let full = decide_complete_core_policy_patch_frame(
            CompleteCorePolicyPatchApplyView {
                gate: GateView {
                    scope: &scope,
                    control,
                    outcomes: &outcomes,
                },
                revision: control.position().get(),
                policies: &policies,
                active_migrations: &migrations,
                completed_migrations: &migrations,
                history_max_events_limit: Err(ApplyError::InvalidAuthority),
                outcome_retention_ms: Err(ApplyError::InvalidAuthority),
            },
            &body,
            log(u64::from(class != 0)),
            schema,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let verified = prepare_complete_core_policy_patch_point_frame(
            &body,
            schema,
            AdmissionLimits::default(),
            &mut charge,
            512 * 1_048_576,
        )?;
        let CompleteCorePolicyPatchPointInspection::Complete(point) =
            inspect_complete_core_policy_patch_point(
                verified,
                log(u64::from(class != 0)),
                PointGateView {
                    scope: &scope,
                    control,
                    retained_nonempty: true,
                    outcome: Some(original),
                },
                (schema, AdmissionLimits::default()),
                &mut charge,
                512 * 1_048_576,
            )?
        else {
            return Err("retained point reached current decode".into());
        };
        assert_eq!(charge.p(), 0);
        for plan in [full, *point] {
            classification(plan.decision(), class, original)?;
            assert_eq!(plan.workflow_type(), None);
            assert_eq!(plan.used_history_max_events_limit(), None);
            assert!(plan.matches_predecessor(None));
            assert_eq!(plan.control().is_log_replay(), class == 0);
            let changes = GroupCorePolicyChanges::new(
                &scope,
                control,
                None,
                plan,
                (schema, AdmissionLimits::default()),
                &mut charge,
                100_000_000,
            )?;
            assert_eq!(changes.control().is_none(), class == 0);
        }
        assert_eq!(original.response_version(), version);
        assert_eq!(
            original.response(),
            if version == 1 {
                COMPLETE.to_vec()
            } else {
                super::original(1)?
            }
        );
    }
    Ok(())
}

fn orders() -> OriginalPolicySetMapOrders<'static> {
    OriginalPolicySetMapOrders::new(
        Sha256::digest(PATCH).into(),
        None,
        &[(0, &[], &[])],
        &[],
        &[],
    )
}

fn original_frame(now: u64, replace: bool) -> TestResult<Vec<u8>> {
    let schema = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let option = [
        Raw::Atom("replace"),
        Raw::Atom(if replace { "true" } else { "false" }),
    ];
    let options = [Raw::Tuple(&option)];
    let prepared = prepare_original_policy_set_request(
        &Raw::Binary(b"r"),
        &Raw::List(&options),
        schema,
        &mut charge,
    )?
    .map_err(|reason| format!("original fixture refused: {reason:?}"))?;
    let facts = orders();
    let wanted = metadata(RetainedStatus::Accepted)?;
    Ok(prepare_original_policy_set_frame(
        OriginalPolicySetFrameInput::new(
            wanted.scope,
            wanted.request,
            stamp(now)?,
            &prepared,
            &facts,
        ),
        schema,
        AdmissionLimits::default(),
        &mut charge,
    )?
    .as_bytes()
    .to_vec())
}

fn original_retained() -> TestResult {
    let schema = profile(100_000_000)?;
    let scope = metadata(RetainedStatus::Accepted)?.scope;
    let encoded = original_frame(10, false)?;
    let verified = prepare_original_policy_set_point_frame_bounded(
        &encoded,
        schema,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(schema),
        512 * 1_048_576,
    )?;
    let header = verified.header(log(0));
    let control =
        ControlState::restored(stamp(10)?, header.identity, EntryStamp::Stamped(stamp(10)?))?;
    let mut wanted = metadata(RetainedStatus::Accepted)?;
    wanted.intent = header.intent;
    wanted.original_position = log(0);
    wanted.original_time = TimestampMillis::new(10);
    wanted.expires_at = TimestampMillis::new(110);
    let original = RetainedOutcome::new(wanted, 1, original(1)?, AdmissionLimits::default())?;
    let outcomes = BTreeMap::from([(original.request().clone(), original)]);
    let original = outcomes.values().next().ok_or("original intent4 receipt")?;
    let policies = BTreeMap::from([(b"r".to_vec(), OriginalPolicyReadValue::Binary(COMPLETE))]);
    let migrations = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::InvalidAuthority))]);
    let facts = orders();
    for class in 0..4 {
        let body = original_frame(if class == 3 { 110 } else { 10 }, class == 2)?;
        let mut charge = PolicySchemaCharge::new(schema);
        let full = decide_original_policy_set_frame(
            OriginalPolicySetApplyView::new(
                OriginalCompletePolicyReplaceApplyView {
                    gate: GateView {
                        scope: &scope,
                        control,
                        outcomes: &outcomes,
                    },
                    revision: control.position().get(),
                    policies: &policies,
                    active_migrations: &migrations,
                    completed_migrations: &migrations,
                    history_max_events_limit: Err(ApplyError::InvalidAuthority),
                    outcome_retention_ms: Err(ApplyError::InvalidAuthority),
                    safety: canonical_original_etf_safety(),
                },
                &facts,
            ),
            &body,
            log(u64::from(class != 0)),
            schema,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        // Existing Full restoration owns E183 + capture64 + four IDs516 + I21.
        assert_eq!(body.len(), 183);
        let full_peak = 183 + 64 + 4 * (128 + 1) + 21;
        assert_eq!(charge.p(), full_peak);
        let verified = prepare_original_policy_set_point_frame_bounded(
            &body,
            schema,
            AdmissionLimits::default(),
            &mut charge,
            512 * 1_048_576,
        )?;
        let OriginalPolicySetPointInspection::Complete(point) =
            inspect_original_policy_set_point_frame_bounded(
                PointGateView {
                    scope: &scope,
                    control,
                    retained_nonempty: true,
                    outcome: Some(original),
                },
                verified,
                log(u64::from(class != 0)),
                schema,
                AdmissionLimits::default(),
                &mut charge,
                512 * 1_048_576,
            )?
        else {
            return Err("intent4 replay reached Fresh current".into());
        };
        // Point's independent 720 header bound stays below the prior Full peak.
        assert_eq!(charge.p(), full_peak);
        for plan in [full, *point] {
            classification(plan.decision(), class, original)?;
            assert_eq!(plan.workflow_type(), None);
            assert!(!plan.matches_predecessor(None));
            assert_eq!(
                (
                    plan.portable_profile_id(),
                    plan.used_history_max_events_limit(),
                    plan.used_migration_generations()
                ),
                (None, None, None)
            );
            let changes = GroupOriginalPolicyChanges::new(
                &scope,
                control,
                None,
                plan,
                (AdmissionLimits::default(), schema),
                &mut charge,
                100_000_000,
            )?;
            assert_eq!(changes.control().is_none(), class == 0);
        }
        assert_eq!(
            (original.intent().version(), original.response_version()),
            (4, 1)
        );
        assert_eq!(original.response(), super::original(1)?);
    }
    Ok(())
}

#[test]
fn interop_nonfresh_replays_original_response_version_before_current_format() -> TestResult {
    for version in [1, 2] {
        core(version)?;
    }
    original_retained()
}
