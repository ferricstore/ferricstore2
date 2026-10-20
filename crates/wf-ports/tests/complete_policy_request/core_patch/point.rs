//! One actual point gate separates retained classification from Fresh ownership.

use super::prepare::cases::{core_patch_prepare_literal_frame, decode_core_patch_literal};
use super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;
use super::response::{business_messages, full_value, intent, minimal_value, patched_value};
use crate::{TestResult, minimal_data, profile};
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError as Error, CompleteCorePolicyPatchFreshView,
    CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
    restore_complete_policy,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{
    DispatchDecision, PolicyRejection, RequestScope, RetainedMetadata, RetainedOutcome,
    RetainedStatus,
};
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, CompleteCorePolicyPatchRequestPlan,
    FreshCompleteCorePolicyPatchFrame, complete_core_policy_patch_response_matches,
    decide_complete_core_policy_patch_point_fresh, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::{GroupCorePolicyChanges, StateTransactionError};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity, TimestampMillis,
};

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn stamp(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}
fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}
fn frame(case: usize) -> TestResult<Vec<u8>> {
    decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[case].0)
}
fn whole_workspace(schema: PolicySchemaLimits, bytes: usize, states: usize) -> TestResult<usize> {
    Ok(usize::try_from(schema.max_work_bytes())?
        + 4096
        + 4 * bytes
        + 2048 * states
        + 56_846_336
        + 263_168
        + 1024)
}
fn authority(current: Option<&CompletePolicyValue>) -> CompleteCorePolicyPatchFreshView<'_> {
    CompleteCorePolicyPatchFreshView {
        workflow_type: b"r",
        current: Ok(current),
        active_migration: Ok(None),
        completed_migration: Ok(None),
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Ok(100),
    }
}
fn fresh<'input>(
    encoded: &'input [u8],
    scope: &RequestScope,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> TestResult<FreshCompleteCorePolicyPatchFrame<'input>> {
    let verified = prepare_complete_core_policy_patch_point_frame(
        encoded,
        schema,
        AdmissionLimits::default(),
        charge,
        workspace,
    )?;
    match inspect_complete_core_policy_patch_point(
        verified,
        log(0),
        PointGateView {
            scope,
            control: ControlState::initial(stamp(0)?),
            retained_nonempty: false,
            outcome: None,
        },
        (schema, AdmissionLimits::default()),
        charge,
        workspace,
    )? {
        CompleteCorePolicyPatchPointInspection::Fresh(value) => Ok(value),
        CompleteCorePolicyPatchPointInspection::Complete(_) => {
            Err("initial request was not Fresh".into())
        }
    }
}
fn plan<'retained>(
    encoded: &[u8],
    scope: &RequestScope,
    current: Option<&CompletePolicyValue>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompleteCorePolicyPatchRequestPlan<'retained>> {
    let workspace = whole_workspace(schema, encoded.len(), 2)?;
    Ok(decide_complete_core_policy_patch_point_fresh(
        fresh(encoded, scope, schema, charge, workspace)?,
        authority(current),
        charge,
    )?)
}

#[test]
fn complete_core_patch_point_gate_runs_once_before_fresh_decode() -> TestResult {
    let schema = profile(20_000_000)?;
    let scope = scope()?;
    let bytes = frame(0)?;
    let workspace = whole_workspace(schema, bytes.len(), 0)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let decoded = fresh(&bytes, &scope, schema, &mut charge, workspace)?;
    assert_eq!(decoded.decoded().encoded().as_ptr(), bytes.as_ptr());
    assert_eq!(decoded.decoded().input().command().workflow_type, b"r");
    // Verify4423 + paid point519 + actual second borrowed count walk1853 + decode1077.
    assert_eq!((charge.g(), charge.p()), (7872, 0));
    let base = decoded.base_control();
    let control = decoded.control();
    let result =
        decide_complete_core_policy_patch_point_fresh(decoded, authority(None), &mut charge)?;
    assert_eq!((charge.g(), charge.p()), (13_031, 0));
    assert_eq!((result.base_control(), result.control()), (base, control));
    assert_eq!(result.expected_revision(), base.position().get());
    let DispatchDecision::Fresh((Some((expected, value)), outcome)) = result.decision() else {
        return Err("missing initial owned policy/outcome".into());
    };
    assert_eq!((*expected, value.generation()), (0, 1));
    assert_eq!(value.encoded(), minimal_value());
    assert_eq!(outcome.response(), minimal_value());
    assert_eq!(outcome.original_position(), log(0));
    assert_eq!(
        (outcome.original_time().get(), outcome.expires_at().get()),
        (0, 100)
    );
    assert_nonfresh_without_decoder()?;
    Ok(())
}

fn assert_nonfresh_without_decoder() -> TestResult {
    let schema = profile(20_000_000)?;
    let scope = scope()?;
    let original_bytes = frame(0)?;
    let identity = EntryIdentity::new(
        log(0),
        PreparedDigest::new(Sha256::digest(&original_bytes).into()),
    );
    let control = ControlState::restored(stamp(0)?, identity, EntryStamp::Stamped(stamp(0)?))?;
    let intent = intent()?;
    for (error, response) in [
        (None, minimal_value()),
        (
            Some(business_messages()[5].0),
            business_messages()[5].1.to_vec(),
        ),
    ] {
        let status = match error {
            None => RetainedStatus::Accepted,
            Some(error) => RetainedStatus::PolicyRejected(PolicyRejection::new(error)?),
        };
        let original = RetainedOutcome::new(
            RetainedMetadata {
                scope: scope.clone(),
                request: RequestId::new(b"q".to_vec())?,
                intent,
                original_position: log(0),
                original_time: TimestampMillis::new(0),
                expires_at: TimestampMillis::new(100),
                status,
            },
            1,
            response,
            AdmissionLimits::default(),
        )?;
        for class in 0..4 {
            let bytes = if class == 2 {
                frame(1)?
            } else {
                core_patch_prepare_literal_frame(
                    &original_bytes[83..],
                    [b"t", b"g", b"r", b"q"],
                    stamp(if class == 3 { 100 } else { 0 })?,
                )?
            };
            let workspace = usize::try_from(schema.max_work_bytes())? + 2048 + 3 * bytes.len();
            let mut charge = PolicySchemaCharge::new(schema);
            let verified = prepare_complete_core_policy_patch_point_frame(
                &bytes,
                schema,
                AdmissionLimits::default(),
                &mut charge,
                workspace,
            )?;
            let classification = inspect_complete_core_policy_patch_point(
                verified,
                log(if class == 0 { 0 } else { 1 }),
                PointGateView {
                    scope: &scope,
                    control,
                    retained_nonempty: true,
                    outcome: Some(&original),
                },
                (schema, AdmissionLimits::default()),
                &mut charge,
                workspace,
            )?;
            let CompleteCorePolicyPatchPointInspection::Complete(plan) = classification else {
                return Err("retained call reconstructed Fresh command".into());
            };
            let plan: CompleteCorePolicyPatchRequestPlan<'_> = plan;
            match (class, plan.decision()) {
                (0, DispatchDecision::LogReplay) => assert_eq!(plan.control().next(), control),
                (1, DispatchDecision::Replay(outcome)) => {
                    let outcome: &RetainedOutcome = outcome;
                    assert!(std::ptr::eq(outcome, &original));
                    assert_eq!(outcome.response().as_ptr(), original.response().as_ptr());
                    assert_eq!(
                        (outcome.original_time().get(), outcome.expires_at().get()),
                        (0, 100)
                    );
                }
                (2, DispatchDecision::IntentConflict) | (3, DispatchDecision::Expired) => {}
                _ => return Err("wrong retained classification".into()),
            }
            let fee = [4813, 5658, 5682, 5658][class];
            assert_eq!((charge.g(), charge.p()), (fee, 0));
        }
    }
    Ok(())
}

#[test]
fn complete_core_patch_point_receipts_keep_full_fwpv2_and_rejection_literals() -> TestResult {
    let schema = profile(100_000_000)?;
    let scope = scope()?;
    let mut charge = PolicySchemaCharge::new(schema);
    let old = decode_complete_policy_value_v2(&full_value()?, schema, &mut charge)?;
    let old_bytes = old.encoded().to_vec();
    let bytes = frame(7)?;
    let result = plan(&bytes, &scope, Some(&old), schema, &mut charge)?;
    let DispatchDecision::Fresh((Some((expected, value)), outcome)) = result.decision() else {
        return Err("missing complete patched value".into());
    };
    assert_eq!((*expected, value.generation()), (1, 2));
    assert_eq!(value.encoded(), patched_value()?);
    assert_eq!(outcome.response(), patched_value()?);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(outcome.intent().version(), 2);
    assert_eq!(value.data().version, old.data().version);
    assert_eq!(
        value.data().indexed_attributes,
        old.data().indexed_attributes
    );
    assert_eq!(
        value.data().indexed_state_meta,
        old.data().indexed_state_meta
    );
    assert!(value.data().governance == old.data().governance);
    assert!(value.data().states.get(b"s".as_slice()) == old.data().states.get(b"s".as_slice()));
    assert_eq!(old.encoded(), old_bytes);
    let bytes = frame(12)?;
    let result = plan(&bytes, &scope, None, schema, &mut charge)?;
    let DispatchDecision::Fresh((None, outcome)) = result.decision() else {
        return Err("invalid version was not a business rejection".into());
    };
    assert_eq!(outcome.response(), business_messages()[0].1);
    assert_eq!(
        outcome.status(),
        RetainedStatus::PolicyRejected(PolicyRejection::new(business_messages()[0].0)?)
    );
    assert_eq!(
        (outcome.original_position(), outcome.expires_at().get()),
        (log(0), 100)
    );
    Ok(())
}

#[test]
fn complete_core_patch_decoder_and_workspace_admit_before_ownership() -> TestResult {
    let schema = profile(20_000_000)?;
    let scope = scope()?;
    let pg = usize::try_from(schema.max_work_bytes())?;
    for (case, states, restoration) in [(0, 0, 1077), (5, 1, 1596), (7, 2, 3705), (9, 4, 3543)] {
        let bytes = frame(case)?;
        let dh = 2048 + 3 * bytes.len();
        let d = 4096 + 4 * bytes.len() + 2048 * states;
        let mut charge = PolicySchemaCharge::new(schema);
        assert_eq!(
            prepare_complete_core_policy_patch_point_frame(
                &bytes,
                schema,
                AdmissionLimits::default(),
                &mut charge,
                pg + dh - 1
            )
            .err(),
            Some(Error::Workspace {
                bytes: pg + dh,
                maximum: pg + dh - 1
            })
        );
        assert_eq!((charge.g(), charge.p()), (0, 0));
        let mut prior = minimal_data();
        prior.indexed_attributes = Some(vec![b"a".to_vec()]);
        let held = restore_complete_policy(1, prior, schema, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1928, 64));
        let verified = prepare_complete_core_policy_patch_point_frame(
            &bytes,
            schema,
            AdmissionLimits::default(),
            &mut charge,
            pg + dh,
        )?;
        let result = inspect_complete_core_policy_patch_point(
            verified,
            log(0),
            PointGateView {
                scope: &scope,
                control: ControlState::initial(stamp(0)?),
                retained_nonempty: false,
                outcome: None,
            },
            (schema, AdmissionLimits::default()),
            &mut charge,
            pg + d - 1,
        );
        assert_eq!(
            result.err(),
            Some(Error::Workspace {
                bytes: pg + d,
                maximum: pg + d - 1
            })
        );
        let verify = FROZEN_CORE_PATCH_FRAMES[case].3;
        let scan = verify - (2435 + bytes.len() as u64);
        assert_eq!((charge.g(), charge.p()), (1928 + verify + 519 + scan, 64));
        assert_eq!(held.data().indexed_attributes, Some(vec![b"a".to_vec()]));
        let mut charge = PolicySchemaCharge::new(schema);
        let decoded = fresh(&bytes, &scope, schema, &mut charge, pg + d)?;
        assert_eq!(
            (charge.g(), charge.p()),
            (verify + 519 + scan + restoration, 0)
        );
        let before = (charge.g(), charge.p());
        let required = whole_workspace(schema, bytes.len(), states)?;
        // The phase admits D exactly, but cannot prepare candidate+response yet.
        assert_eq!(
            decide_complete_core_policy_patch_point_fresh(decoded, authority(None), &mut charge)
                .err(),
            Some(Error::Workspace {
                bytes: required,
                maximum: pg + d
            })
        );
        assert_eq!((charge.g(), charge.p()), before);
        let mut charge = PolicySchemaCharge::new(schema);
        let decoded = fresh(&bytes, &scope, schema, &mut charge, required - 1)?;
        let before = (charge.g(), charge.p());
        assert_eq!(
            decide_complete_core_policy_patch_point_fresh(decoded, authority(None), &mut charge)
                .err(),
            Some(Error::Workspace {
                bytes: required,
                maximum: required - 1
            })
        );
        assert_eq!((charge.g(), charge.p()), before);
        assert_workspace_positive(&bytes, &scope, schema, required, case)?;
    }
    Ok(())
}

fn assert_workspace_positive(
    bytes: &[u8],
    scope: &RequestScope,
    schema: PolicySchemaLimits,
    required: usize,
    case: usize,
) -> TestResult {
    let mut charge = PolicySchemaCharge::new(schema);
    let decoded = fresh(bytes, scope, schema, &mut charge, required)?;
    let completed =
        decide_complete_core_policy_patch_point_fresh(decoded, authority(None), &mut charge)?;
    match completed.decision() {
        DispatchDecision::Fresh((None, outcome)) if case == 5 || case == 9 => {
            assert_eq!(outcome.response(), business_messages()[3].1);
            assert_eq!(
                outcome.status(),
                RetainedStatus::PolicyRejected(PolicyRejection::new(business_messages()[3].0)?)
            );
            assert_eq!(
                (outcome.original_time().get(), outcome.expires_at().get()),
                (0, 100)
            );
        }
        DispatchDecision::Fresh((Some(_), outcome)) if case == 0 || case == 7 => {
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
        }
        _ => return Err("wrong independently expected exact-workspace result".into()),
    }
    Ok(())
}

#[test]
fn complete_core_patch_changes_bind_current_control_policy_and_response() -> TestResult {
    let schema = profile(100_000_000)?;
    let scope = scope()?;
    let bytes = frame(0)?;
    let expected = ControlState::initial(stamp(0)?);
    let mut charge = PolicySchemaCharge::new(schema);
    let old = decode_complete_policy_value_v2(&full_value()?, schema, &mut charge)?;
    for case in 0..5 {
        let plan = plan(&bytes, &scope, None, schema, &mut charge)?;
        let foreign = RequestScope::new(
            TenantId::new(b"u".to_vec())?,
            scope.group().clone(),
            scope.recovery().clone(),
        );
        let scope_arg = if case == 1 { &foreign } else { &scope };
        let base = if case == 2 {
            ControlState::initial(stamp(1)?)
        } else {
            expected
        };
        let predecessor = if case == 3 { Some(&old) } else { None };
        let mut wrong_account = PolicySchemaCharge::new(profile(100_000_001)?);
        let account = if case == 4 {
            &mut wrong_account
        } else {
            &mut charge
        };
        let changes = GroupCorePolicyChanges::new(
            scope_arg,
            base,
            predecessor,
            plan,
            (schema, AdmissionLimits::default()),
            account,
            100_000,
        );
        if case == 0 {
            let changes = changes?;
            assert_eq!(changes.scope(), &scope);
            assert_eq!(changes.expected(), expected);
            assert!(changes.predecessor().is_none());
            assert_eq!(changes.plan().base_control(), expected);
        } else {
            assert_eq!(changes.err(), Some(StateTransactionError::InvalidChanges));
        }
        if case == 4 {
            assert_eq!((wrong_account.g(), wrong_account.p()), (0, 0));
        }
    }
    assert_response_correspondence(schema)?;
    Ok(())
}

fn assert_response_correspondence(schema: PolicySchemaLimits) -> TestResult {
    let scope = scope()?;
    let bytes = frame(0)?;
    for case in 0..5 {
        let mut charge = PolicySchemaCharge::new(schema);
        let (_, _, _, decision) = plan(&bytes, &scope, None, schema, &mut charge)?.into_parts();
        let DispatchDecision::Fresh((mut candidate, original)) = decision else {
            return Err("missing independent Fresh fixture".into());
        };
        let mut response = original.response().to_vec();
        let mut status = original.status();
        if case == 1 {
            response = full_value()?;
        }
        if case == 2 {
            status =
                RetainedStatus::PolicyRejected(PolicyRejection::new(business_messages()[5].0)?);
        }
        if case == 3 {
            candidate = None;
        }
        if case == 4 {
            candidate = Some((
                0,
                decode_complete_policy_value_v2(&full_value()?, schema, &mut charge)?,
            ));
        }
        let outcome = RetainedOutcome::new(
            RetainedMetadata {
                scope: scope.clone(),
                request: original.request().clone(),
                intent: original.intent(),
                original_position: original.original_position(),
                original_time: original.original_time(),
                expires_at: original.expires_at(),
                status,
            },
            1,
            response,
            AdmissionLimits::default(),
        )?;
        assert_eq!(
            complete_core_policy_patch_response_matches(
                &(candidate, outcome),
                (schema, AdmissionLimits::default()),
                &mut charge,
                1_000_000
            )?,
            case == 0
        );
    }
    // Binding admits lengths and DATA independently; it supplies no wire proof.
    // Exercise both generation and DATA disagreement through public constructors.
    let mut mismatches = [false; 2];
    for (index, generation) in [2, 1].into_iter().enumerate() {
        let mut charge = PolicySchemaCharge::new(schema);
        let (_, _, _, decision) = plan(&bytes, &scope, None, schema, &mut charge)?.into_parts();
        let DispatchDecision::Fresh((_, outcome)) = decision else {
            return Err("missing original correspondence receipt".into());
        };
        let mut data = minimal_data();
        if index == 1 {
            data.workflow_type = b"different".to_vec();
        }
        let candidate = restore_complete_policy(generation, data, schema, &mut charge)?;
        let response = outcome.response().to_vec();
        let value =
            candidate.bind_encoded_snapshot(18..response.len(), response, schema, &mut charge)?;
        mismatches[index] = complete_core_policy_patch_response_matches(
            &(Some((generation - 1, value)), outcome),
            (schema, AdmissionLimits::default()),
            &mut charge,
            1_000_000,
        )?;
    }
    assert_eq!(mismatches, [false, false]);
    Ok(())
}
