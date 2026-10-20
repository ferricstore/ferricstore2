//! Full and point builders bind actual raw predecessors and reached context.

use super::{BIG_VERSION, COMPLETE, TestResult, frame, log, metadata, original, profile, stamp};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError as Error, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchFreshView, PolicySchemaCharge, PolicySchemaLimits,
    decode_complete_policy_value_v2,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
use wf_domain::policy::installation::PolicyError;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::{GateView, PointGateView};
use wf_domain::workflow::request::{DispatchDecision, PolicyRejection, RetainedStatus};
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, CompleteCorePolicyPatchRequestPlan,
    complete_core_policy_patch_response_matches, decide_complete_core_policy_patch_frame,
    decide_complete_core_policy_patch_point_fresh, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::{GroupCorePolicyChanges, StateTransactionError, StoredPolicyValue};
use wf_types::identity::TenantId;

fn bytes(value: &StoredPolicyValue) -> &[u8] {
    match value {
        StoredPolicyValue::Complete(value) => value.encoded(),
        StoredPolicyValue::Original(raw) => raw,
    }
}

fn current(case: usize, schema: PolicySchemaLimits) -> TestResult<Option<StoredPolicyValue>> {
    Ok(match case {
        0 => None,
        1 => Some(StoredPolicyValue::Complete(Box::new(
            decode_complete_policy_value_v2(
                COMPLETE,
                schema,
                &mut PolicySchemaCharge::new(schema),
            )?,
        ))),
        2 => Some(StoredPolicyValue::Original(original(1)?)),
        3 => Some(StoredPolicyValue::Original(BIG_VERSION.to_vec())),
        _ => return Err("unselected authority fixture".into()),
    })
}

fn expected(case: usize) -> TestResult<Vec<u8>> {
    if case <= 1 {
        let mut raw = COMPLETE.to_vec();
        raw[6..14].copy_from_slice(&(if case == 0 { 1_u64 } else { 2 }).to_be_bytes());
        Ok(raw)
    } else if case == 2 {
        original(2)
    } else {
        let mut raw = BIG_VERSION.to_vec();
        assert_eq!(&raw[19..21], &[97, 1]);
        raw[20] = 2;
        Ok(raw)
    }
}

fn point<'retained>(
    encoded: &[u8],
    current: Option<&StoredPolicyValue>,
    history: Result<u32, ApplyError>,
    query: &[u8],
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompleteCorePolicyPatchRequestPlan<'retained>> {
    let schema = charge.profile();
    let scope = metadata(RetainedStatus::Accepted)?.scope;
    let verified = prepare_complete_core_policy_patch_point_frame(
        encoded,
        schema,
        AdmissionLimits::default(),
        charge,
        512 * 1_048_576,
    )?;
    let CompleteCorePolicyPatchPointInspection::Fresh(fresh) =
        inspect_complete_core_policy_patch_point(
            verified,
            log(0),
            PointGateView {
                scope: &scope,
                control: ControlState::initial(stamp(0)?),
                retained_nonempty: false,
                outcome: None,
            },
            (schema, AdmissionLimits::default()),
            charge,
            512 * 1_048_576,
        )?
    else {
        return Err("new request did not enter Fresh".into());
    };
    let query_owner = fresh.decoded().input().command().workflow_type.as_ptr();
    let original = matches!(current, Some(StoredPolicyValue::Original(_)));
    let plan = decide_complete_core_policy_patch_point_fresh(
        *fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: query,
            current: Ok(current),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: history,
            outcome_retention_ms: Ok(100),
        },
        charge,
    )?;
    if original {
        assert_eq!(plan.workflow_type().map(<[u8]>::as_ptr), Some(query_owner));
    }
    Ok(plan)
}

fn accepted(
    plan: &CompleteCorePolicyPatchRequestPlan<'_>,
    old: Option<&StoredPolicyValue>,
    case: usize,
) -> TestResult {
    let scope = metadata(RetainedStatus::Accepted)?.scope;
    assert_eq!(plan.base_control(), ControlState::initial(stamp(0)?));
    assert_eq!(
        plan.expected_revision(),
        plan.base_control().position().get()
    );
    assert_eq!(plan.workflow_type(), Some(b"r".as_slice()));
    assert_eq!(plan.used_history_max_events_limit(), Some(64));
    assert!(plan.matches_predecessor(old));
    let DispatchDecision::Fresh((Some((generation, value)), outcome)) = plan.decision() else {
        return Err("full stored result was not accepted".into());
    };
    assert_eq!(*generation, u64::from(case != 0));
    assert_eq!(bytes(value), expected(case)?);
    assert_eq!(outcome.response(), expected(case)?);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.intent().version(), 2);
    assert_eq!(outcome.response_version(), if case <= 1 { 1 } else { 2 });
    assert_eq!(outcome.scope(), &scope);
    assert_eq!(outcome.request().as_bytes(), b"q");
    assert_eq!(
        (
            outcome.original_position(),
            outcome.original_time().get(),
            outcome.expires_at().get()
        ),
        (log(0), 10, 110)
    );
    Ok(())
}

#[test]
fn stored_full_and_point_fresh_share_actual_format_and_context_authority() -> TestResult {
    let schema = profile(100_000_000)?;
    let scope = metadata(RetainedStatus::Accepted)?.scope;
    let encoded = frame(10, None, PolicyPatch::default())?;
    for case in 0..4 {
        let mut policies = BTreeMap::new();
        if let Some(value) = current(case, schema)? {
            policies.insert(b"r".to_vec(), Ok(value));
        }
        let old = policies
            .get(b"r".as_slice())
            .and_then(|result| result.as_ref().ok());
        let saved = old.map(bytes).map(<[u8]>::to_vec);
        let outcomes = BTreeMap::new();
        let migrations = BTreeMap::new();
        let mut charge = PolicySchemaCharge::new(schema);
        let full = decide_complete_core_policy_patch_frame(
            CompleteCorePolicyPatchApplyView {
                gate: GateView {
                    scope: &scope,
                    control: ControlState::initial(stamp(0)?),
                    outcomes: &outcomes,
                },
                revision: 0,
                policies: &policies,
                active_migrations: &migrations,
                completed_migrations: &migrations,
                history_max_events_limit: Ok(64),
                outcome_retention_ms: Ok(100),
            },
            &encoded,
            log(0),
            schema,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let point = point(&encoded, old, Ok(64), b"r", &mut charge)?;
        for plan in [full, point] {
            accepted(&plan, old, case)?;
            let before = (charge.g(), charge.p());
            assert_eq!(plan.workflow_type(), Some(b"r".as_slice()));
            assert_eq!((charge.g(), charge.p()), before);
            let DispatchDecision::Fresh(fresh) = plan.decision() else {
                return Err("Fresh result".into());
            };
            assert!(complete_core_policy_patch_response_matches(
                fresh,
                (schema, AdmissionLimits::default()),
                &mut charge,
                100_000_000
            )?);
            let changes = GroupCorePolicyChanges::new(
                &scope,
                plan.base_control(),
                old,
                plan,
                (schema, AdmissionLimits::default()),
                &mut charge,
                100_000_000,
            )?;
            assert_eq!(changes.plan().workflow_type(), Some(b"r".as_slice()));
        }
        assert_eq!(old.map(bytes), saved.as_deref());
    }
    assert_predecessor_seals(schema)?;
    assert_reached_context(schema)?;
    Ok(())
}

fn assert_predecessor_seals(schema: PolicySchemaLimits) -> TestResult {
    let scope = metadata(RetainedStatus::Accepted)?.scope;
    let old = current(2, schema)?.ok_or("original predecessor")?;
    let synthesized = current(1, schema)?.ok_or("V2 predecessor")?;
    // Same type/generation/content, distinct physical original sibling order.
    let alternate = StoredPolicyValue::Original(
        b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x01\
        \x74\0\0\0\x02\x77\x04type\x6d\0\0\0\x01r\x77\x06states\x74\0\0\0\0"
            .to_vec(),
    );
    let encoded = frame(10, None, PolicyPatch::default())?;
    for case in 0..6 {
        let mut charge = PolicySchemaCharge::new(schema);
        let plan = point(&encoded, Some(&old), Ok(64), b"r", &mut charge)?;
        assert!(!plan.matches_predecessor(None));
        assert!(!plan.matches_predecessor(Some(&alternate)));
        assert!(!plan.matches_predecessor(Some(&synthesized)));
        let foreign = wf_domain::workflow::request::RequestScope::new(
            TenantId::new(b"other".to_vec())?,
            scope.group().clone(),
            scope.recovery().clone(),
        );
        let predecessor = match case {
            0 => None,
            1 => Some(&alternate),
            2 => Some(&synthesized),
            _ => Some(&old),
        };
        let expected = if case == 4 {
            ControlState::initial(stamp(1)?)
        } else {
            plan.base_control()
        };
        let mut wrong = PolicySchemaCharge::new(profile(100_000_001)?);
        let account = if case == 5 { &mut wrong } else { &mut charge };
        assert_eq!(
            GroupCorePolicyChanges::new(
                if case == 3 { &foreign } else { &scope },
                expected,
                predecessor,
                plan,
                (schema, AdmissionLimits::default()),
                account,
                100_000_000
            )
            .err(),
            Some(StateTransactionError::InvalidChanges)
        );
        assert_eq!((wrong.g(), wrong.p()), (0, 0));
    }
    Ok(())
}

fn assert_reached_context(schema: PolicySchemaLimits) -> TestResult {
    let old = current(2, schema)?.ok_or("original predecessor")?;
    let mut charge = PolicySchemaCharge::new(schema);
    let stale = frame(10, Some(2), PolicyPatch::default())?;
    let plan = point(
        &stale,
        Some(&old),
        Err(ApplyError::InvalidAuthority),
        b"r",
        &mut charge,
    )?;
    assert_eq!(plan.used_history_max_events_limit(), None);
    assert!(plan.matches_predecessor(Some(&old)));
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("stale business response".into());
    };
    assert_eq!(
        outcome.status(),
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?)
    );
    assert_eq!(outcome.response_version(), 1);
    let encoded = frame(
        10,
        Some(1),
        PolicyPatch {
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(65),
            }),
            ..PolicyPatch::default()
        },
    )?;
    let plan = point(&encoded, Some(&old), Ok(64), b"r", &mut charge)?;
    assert_eq!(plan.used_history_max_events_limit(), Some(64));
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("history rejection".into());
    };
    assert_eq!(
        outcome.response(),
        b"ERR flow retention history_max_events is outside configured bounds"
    );
    assert_eq!(outcome.response_version(), 1);
    for (query, history) in [
        (b"r".as_slice(), Err(ApplyError::InvalidAuthority)),
        (b"other", Ok(64)),
    ] {
        let error = point(&encoded, Some(&old), history, query, &mut charge)
            .err()
            .ok_or("authority refused")?;
        assert_eq!(
            error.downcast_ref::<Error>(),
            Some(&Error::Apply(ApplyError::InvalidAuthority))
        );
    }
    Ok(())
}
