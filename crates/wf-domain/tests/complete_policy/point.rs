//! Point-Fresh composition preserves the existing complete mathematical contract.

use super::{TestResult, data, limits};
use std::collections::BTreeMap;
use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError as Error, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchFresh, CompleteCorePolicyPatchFreshView, CompleteCorePolicyPatchInput,
    CompleteCorePolicyPatchRequestInput, CompletePolicyData, CompletePolicyValue,
    CompleteStatePolicyData, GovernanceData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, StoredPolicyValue, prepare_complete_core_policy_patch_fresh,
    prepare_complete_core_policy_patch_point_fresh, prepare_complete_policy_value_v2,
    restore_complete_core_policy_patch_frame_header, restore_complete_policy,
    scan_complete_core_policy_patch_frame,
};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, PolicyVersion, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::retention::{RetentionError, RetentionOverride};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
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

fn stamp() -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?)
}

fn intent() -> TestResult<ClientIntent> {
    // Existing frozen producer vector; the domain receives its recomputed digest.
    let text = concat!(
        "4657435000010500000001740000000167000000017200000001710002",
        "be444a3a4326b0d998b2e130a02b075b72816ed0c1c5a301d46bb8aea99ecc16",
        "0000000000000000000000000000000000000000003457462d494e54454e5400",
        "434f4d504c4554452d434f52452d50415443480056320000000001740000000001720000000000000000"
    );
    let mut wire = Vec::new();
    for pair in text.as_bytes().as_chunks::<2>().0 {
        wire.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    let scanned = scan_complete_core_policy_patch_frame(
        &wire,
        profile,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let digest = <[u8; 32]>::try_from(&wire[29..61])?;
    Ok(restore_complete_core_policy_patch_frame_header(
        scanned,
        digest,
        profile,
        AdmissionLimits::default(),
        &mut charge,
    )?
    .intent())
}

fn input(
    patch: PolicyPatch,
    expected: Option<u64>,
) -> TestResult<CompleteCorePolicyPatchRequestInput> {
    Ok(CompleteCorePolicyPatchRequestInput::new(
        scope()?,
        RequestId::new(b"q".to_vec())?,
        stamp()?,
        CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: expected,
            patch,
        },
    ))
}

fn stored(value: CompletePolicyData) -> TestResult<StoredPolicyValue> {
    let profile = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(profile);
    let candidate = restore_complete_policy(7, value, profile, &mut charge)?;
    Ok(StoredPolicyValue::Complete(Box::new(
        prepare_complete_policy_value_v2(candidate, profile, &mut charge)?,
    )))
}

fn complete(value: &StoredPolicyValue) -> TestResult<&CompletePolicyValue> {
    match value {
        StoredPolicyValue::Complete(value) => Ok(value),
        StoredPolicyValue::Original(_) => Err("expected Complete fixture".into()),
    }
}

fn preserved() -> CompletePolicyData {
    let mut value = data(
        b"r".to_vec(),
        Some(PolicyVersion::Number(0)),
        BTreeMap::from([(
            b"queued".to_vec(),
            CompleteStatePolicyData {
                mode: Some(StateMode::Parallel),
                retry: Some(RetryOverride::default()),
                retention: Some(RetentionOverride::default()),
                governance: Some(GovernanceData {
                    mode: None,
                    effects: None,
                    audit: None,
                    limits: None,
                    budgets: None,
                    circuits: None,
                    approvals: None,
                }),
            },
        )]),
    );
    value.max_active_ms = Some(1_000);
    value.retry = Some(RetryOverride {
        max_retries: Some(7),
        backoff: None,
        exhausted_to: Some(b"dead".to_vec()),
    });
    value.retention = Some(RetentionOverride {
        ttl_ms: Some(1),
        history_max_events: Some(64),
    });
    value.indexed_attributes = Some(vec![b"a".to_vec(), b"z".to_vec()]);
    value.indexed_state_meta = Some(b"m".to_vec());
    value.governance = Some(GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: None,
        budgets: None,
        circuits: None,
        approvals: None,
    });
    value
}

fn compare(
    point: &CompleteCorePolicyPatchFresh,
    full: &CompleteCorePolicyPatchFresh,
) -> TestResult {
    assert_eq!(point.1, full.1);
    match (&point.0, &full.0) {
        (Some((before, actual)), Some((expected, reference))) => {
            assert_eq!(before, expected);
            let (actual, reference) = (complete(actual)?, complete(reference)?);
            assert_eq!(actual.generation(), reference.generation());
            assert!(actual.data() == reference.data());
            assert_eq!(actual.encoded(), reference.encoded());
            assert_eq!(point.1.response(), actual.encoded());
        }
        (None, None) => {}
        _ => return Err("point and complete reference disagree about installation".into()),
    }
    Ok(())
}

#[test]
fn point_fresh_matches_full_complete_fields_and_order() -> TestResult {
    let original = stored(preserved())?;
    let original_wire = original.encoded().to_vec();
    let policies = BTreeMap::from([(b"r".to_vec(), Ok(original))]);
    let marks = BTreeMap::from([(b"r".to_vec(), Ok(9))]);
    let patch = PolicyPatch {
        max_active_ms: FieldPatch::Set(2_000),
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    let profile = limits(20_000_000)?;
    let mut full_charge = PolicySchemaCharge::new(profile);
    let mut point_charge = PolicySchemaCharge::new(profile);
    let namespace = scope()?;
    let outcomes = BTreeMap::new();
    let current = policies
        .get(b"r".as_slice())
        .ok_or("missing current")?
        .as_ref()
        .map_err(|_| "current fixture refused")?;
    let full_view = CompleteCorePolicyPatchApplyView {
        gate: GateView {
            scope: &namespace,
            control: ControlState::initial(stamp()?),
            outcomes: &outcomes,
        },
        revision: 7,
        policies: &policies,
        active_migrations: &marks,
        completed_migrations: &BTreeMap::new(),
        history_max_events_limit: Ok(64),
        outcome_retention_ms: Ok(1_000),
    };
    let committed = (
        intent()?,
        LogIdentity::new(1, 7, AppliedPosition::new(0)),
        TimestampMillis::new(100),
    );
    let full = prepare_complete_core_policy_patch_fresh(
        full_view,
        input(patch.clone(), Some(7))?,
        committed,
        profile,
        AdmissionLimits::default(),
        &mut full_charge,
    )?;
    let point = prepare_complete_core_policy_patch_point_fresh(
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"r",
            current: Ok(Some(current)),
            active_migration: Ok(Some(9)),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(1_000),
        },
        input(patch, Some(7))?,
        committed,
        profile,
        AdmissionLimits::default(),
        &mut point_charge,
    )?;
    compare(&point, &full)?;
    assert_eq!(
        (point_charge.g(), point_charge.p()),
        (full_charge.g(), full_charge.p())
    );
    let installed = complete(&point.0.as_ref().ok_or("missing candidate")?.1)?;
    let mut expected = preserved();
    expected.max_active_ms = Some(2_000);
    expected
        .states
        .get_mut(b"queued".as_slice())
        .ok_or("missing state")?
        .mode = Some(StateMode::Fifo);
    assert!(installed.data() == &expected);
    assert_eq!(installed.generation(), 10);
    assert_eq!(point.1.expires_at(), TimestampMillis::new(1_100));
    assert_eq!(current.encoded(), original_wire);
    assert_wrong_current()?;
    Ok(())
}

fn assert_wrong_current() -> TestResult {
    let mut wrong = preserved();
    wrong.workflow_type = b"foreign".to_vec();
    let wrong = stored(wrong)?;
    let profile = limits(20_000_000)?;
    let result = prepare_complete_core_policy_patch_point_fresh(
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"r",
            current: Ok(Some(&wrong)),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(1_000),
        },
        input(PolicyPatch::default(), Some(7))?,
        (
            intent()?,
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            TimestampMillis::new(100),
        ),
        profile,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(profile),
    );
    assert!(matches!(result, Err(Error::Request(_))));
    assert_eq!(complete(&wrong)?.data().workflow_type, b"foreign");
    let unbound = prepare_complete_core_policy_patch_point_fresh(
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"foreign",
            current: Ok(None),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(1_000),
        },
        input(PolicyPatch::default(), None)?,
        (
            intent()?,
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            TimestampMillis::new(100),
        ),
        profile,
        AdmissionLimits::default(),
        &mut PolicySchemaCharge::new(profile),
    );
    assert_eq!(
        unbound.err(),
        Some(Error::Apply(ApplyError::InvalidAuthority))
    );
    Ok(())
}

#[test]
fn point_fresh_preserves_cas_context_migration_and_work_precedence() -> TestResult {
    for case in 0..12 {
        assert_precedence(case)?;
    }
    assert_continuous_work()?;
    Ok(())
}

fn facts(case: usize) -> CompleteCorePolicyPatchFreshView<'static> {
    let bad = ApplyError::InvalidAuthority;
    let (history, active, completed, ttl) = match case {
        0 => (Err(bad), Err(bad), Err(bad), Ok(1_000)),
        1 => (Err(bad), Err(bad), Err(bad), Err(bad)),
        2 => (Ok(0), Err(bad), Err(bad), Err(bad)),
        3 => (Ok(64), Err(bad), Err(bad), Ok(1_000)),
        4 => (Ok(64), Err(bad), Err(bad), Err(bad)),
        5 => (
            Ok(64),
            Ok(Some(MAX_POLICY_GENERATION + 1)),
            Err(bad),
            Err(bad),
        ),
        6 => (Ok(64), Ok(None), Err(bad), Err(bad)),
        7 => (
            Ok(64),
            Ok(None),
            Ok(Some(MAX_POLICY_GENERATION + 1)),
            Err(bad),
        ),
        8 => (Ok(64), Ok(Some(MAX_POLICY_GENERATION)), Ok(None), Ok(1_000)),
        9 => (Ok(64), Ok(None), Ok(None), Err(bad)),
        10 => (Ok(64), Ok(None), Ok(None), Ok(0)),
        _ => (Ok(64), Ok(Some(6)), Ok(Some(7)), Ok(1_000)),
    };
    CompleteCorePolicyPatchFreshView {
        workflow_type: b"r",
        current: Ok(None),
        active_migration: active,
        completed_migration: completed,
        history_max_events_limit: history,
        outcome_retention_ms: ttl,
    }
}

fn assert_precedence(case: usize) -> TestResult {
    let observed = facts(case);
    let history = observed.history_max_events_limit;
    let active = observed.active_migration;
    let completed = observed.completed_migration;
    let ttl = observed.outcome_retention_ms;
    let mut patch = PolicyPatch::default();
    if case == 3 {
        patch.retention = FieldPatch::Set(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(65),
        });
    }
    let expected = if case == 0 { Some(1) } else { Some(0) };
    let profile = limits(20_000_000)?;
    let namespace = scope()?;
    let policies = BTreeMap::new();
    let active_map = active.map_or_else(
        |e| BTreeMap::from([(b"r".to_vec(), Err(e))]),
        |v| v.map_or_else(BTreeMap::new, |v| BTreeMap::from([(b"r".to_vec(), Ok(v))])),
    );
    let completed_map = completed.map_or_else(
        |e| BTreeMap::from([(b"r".to_vec(), Err(e))]),
        |v| v.map_or_else(BTreeMap::new, |v| BTreeMap::from([(b"r".to_vec(), Ok(v))])),
    );
    let outcomes = BTreeMap::new();
    let view = CompleteCorePolicyPatchApplyView {
        gate: GateView {
            scope: &namespace,
            control: ControlState::initial(stamp()?),
            outcomes: &outcomes,
        },
        revision: 0,
        policies: &policies,
        active_migrations: &active_map,
        completed_migrations: &completed_map,
        history_max_events_limit: history,
        outcome_retention_ms: ttl,
    };
    let committed = (
        intent()?,
        LogIdentity::new(1, 7, AppliedPosition::new(0)),
        TimestampMillis::new(100),
    );
    let mut full_g = PolicySchemaCharge::new(profile);
    let mut point_g = PolicySchemaCharge::new(profile);
    let full = prepare_complete_core_policy_patch_fresh(
        view,
        input(patch.clone(), expected)?,
        committed,
        profile,
        AdmissionLimits::default(),
        &mut full_g,
    );
    let point = prepare_complete_core_policy_patch_point_fresh(
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"r",
            current: Ok(None),
            active_migration: active,
            completed_migration: completed,
            history_max_events_limit: history,
            outcome_retention_ms: ttl,
        },
        input(patch, expected)?,
        committed,
        profile,
        AdmissionLimits::default(),
        &mut point_g,
    );
    assert_eq!(
        (point_g.g(), point_g.p()),
        (full_g.g(), full_g.p()),
        "case {case}"
    );
    match (point, full) {
        (Ok(point), Ok(full)) => {
            compare(&point, &full)?;
            assert_precedence_result(case, &point)?;
        }
        (Err(point), Err(full)) => {
            assert!(
                !matches!(case, 0 | 3 | 8 | 11),
                "expected business/accepted result in case {case}"
            );
            assert_eq!(point, full, "case {case}");
        }
        _ => return Err(format!("point/full precedence diverged in case {case}").into()),
    }
    Ok(())
}

fn assert_precedence_result(case: usize, point: &CompleteCorePolicyPatchFresh) -> TestResult {
    let business = match case {
        0 => Some((
            PolicyError::StaleGeneration,
            b"ERR stale flow policy generation".as_slice(),
        )),
        3 => Some((
            PolicyError::Retention(RetentionError::History),
            b"ERR flow retention history_max_events is outside configured bounds".as_slice(),
        )),
        8 => Some((
            PolicyError::GenerationExhausted,
            b"ERR flow policy generation exhausted".as_slice(),
        )),
        _ => None,
    };
    if let Some((reason, body)) = business {
        assert!(
            matches!(point.1.status(), RetainedStatus::PolicyRejected(r) if r.error() == reason)
        );
        assert_eq!(point.1.response(), body);
        assert!(point.0.is_none());
    } else {
        assert_eq!(case, 11);
        assert_eq!(
            complete(&point.0.as_ref().ok_or("missing candidate")?.1)?.generation(),
            8
        );
    }
    Ok(())
}

fn assert_continuous_work() -> TestResult {
    // Pure minimal Fresh is independent of the frame/gate and has 5,159 work bytes.
    // The existing account prefix is a two-name complete candidate: g=1,928,p=64.
    let target = 1_928 + 5_159 + 64;
    for maximum in [target - 1, target] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let mut prefix = data(b"r".to_vec(), None, BTreeMap::new());
        prefix.indexed_attributes = Some(vec![b"a".to_vec()]);
        let held = restore_complete_policy(1, prefix, profile, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        let result = prepare_complete_core_policy_patch_point_fresh(
            CompleteCorePolicyPatchFreshView {
                workflow_type: b"r",
                current: Ok(None),
                active_migration: Ok(None),
                completed_migration: Ok(None),
                history_max_events_limit: Ok(64),
                outcome_retention_ms: Ok(1_000),
            },
            input(PolicyPatch::default(), None)?,
            (
                intent()?,
                LogIdentity::new(1, 7, AppliedPosition::new(0)),
                TimestampMillis::new(100),
            ),
            profile,
            AdmissionLimits::default(),
            &mut charge,
        );
        if maximum == target {
            assert!(result.is_ok());
            assert_eq!(charge.total(), target);
        } else {
            assert_eq!(
                result.err(),
                Some(Error::Request(
                    PolicySchemaError::WorkBytes {
                        bytes: 7_151,
                        maximum: 7_150
                    }
                    .into()
                ))
            );
            assert!(charge.g() > 1_928);
        }
        assert_eq!(held.generation(), 1);
        assert_eq!(charge.p(), 64);
    }
    Ok(())
}
