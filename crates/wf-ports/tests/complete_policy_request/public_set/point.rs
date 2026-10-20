//! A paid point gate and sealed original inputs precede prospective owners.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, DispatchDecision, ExistingError, GateView,
    OriginalEtfSafety, OriginalPolicySetApplyView, Plan, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, Raw, RawError, ReadValue, RequestId, RetainedOutcome, TestResult, accepted,
    digest, empty_orders, frame, frame_with, hex, log, minimal, profile, retained, scope, stamp,
    view,
};
use wf_domain::policy::complete::{
    OriginalPolicySetFreshView, prepare_original_policy_set_request,
};
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::log::{
    FreshOriginalPolicySetFrame, OriginalPolicySetPointInspection,
    decide_original_policy_set_frame_bounded, decide_original_policy_set_point_fresh_bounded,
    inspect_original_policy_set_point_frame_bounded,
    prepare_original_policy_set_point_frame_bounded,
};

fn workspace(schema: PolicySchemaLimits, encoded: &[u8]) -> TestResult<usize> {
    // Pg + fixed safety arrays + borrowed header; late two ETF/key/receipt owners.
    Ok(usize::try_from(schema.max_work_bytes())?
        + 65_536
        + 2048
        + 3 * encoded.len()
        + 2 * 263_168
        + 65_535
        + 1024)
}

pub(super) fn authority(current: ReadValue<'_>) -> OriginalPolicySetFreshView<'_> {
    OriginalPolicySetFreshView {
        current,
        active_migration: Ok(0),
        completed_migration: Ok(0),
        history_max_events_limit: Ok(64),
        outcome_retention_ms: Ok(100),
    }
}

fn fresh<'frame>(
    encoded: &'frame [u8],
    namespace: &RequestScope,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum: usize,
) -> TestResult<FreshOriginalPolicySetFrame<'frame>> {
    let verified = prepare_original_policy_set_point_frame_bounded(
        encoded,
        schema,
        AdmissionLimits::default(),
        charge,
        maximum,
    )?;
    assert_eq!(verified.encoded().as_ptr(), encoded.as_ptr());
    match inspect_original_policy_set_point_frame_bounded(
        PointGateView {
            scope: namespace,
            control: ControlState::initial(stamp(0)?),
            retained_nonempty: false,
            outcome: None,
        },
        verified,
        log(0),
        schema,
        AdmissionLimits::default(),
        charge,
        maximum,
    )? {
        OriginalPolicySetPointInspection::Fresh(value) => Ok(*value),
        OriginalPolicySetPointInspection::Complete(_) => Err("initial request not Fresh".into()),
    }
}

pub(super) fn point_plan<'retained>(
    encoded: &[u8],
    namespace: &RequestScope,
    current: OriginalPolicySetFreshView<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<Plan<'retained>> {
    let maximum = workspace(schema, encoded)?;
    Ok(decide_original_policy_set_point_fresh_bounded(
        fresh(encoded, namespace, schema, charge, maximum)?,
        current,
        schema,
        AdmissionLimits::default(),
        charge,
        maximum,
    )?)
}

#[test]
fn paid_point_gate_precedes_current_orders_and_context() -> TestResult {
    let schema = profile(2_000_000_000)?;
    let namespace = scope()?;
    let orders = empty_orders();
    let original = frame(Raw::List(&[]), &orders)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let origin = point_plan(
        &original,
        &namespace,
        authority(ReadValue::Absent),
        schema,
        &mut charge,
    )?;
    accepted(&origin, 0, 1, &minimal(1)?)?;
    let (_, _, control, decision) = origin.into_parts();
    let DispatchDecision::Fresh((_, outcome)) = decision else {
        return Err("original retained owner".into());
    };
    for case in 0..4 {
        assert_nonfresh(case, &original, control.next(), &outcome, schema)?;
    }
    Ok(())
}

fn assert_nonfresh(
    case: usize,
    original: &[u8],
    control: ControlState,
    outcome: &RetainedOutcome,
    schema: PolicySchemaLimits,
) -> TestResult {
    let namespace = scope()?;
    let orders = empty_orders();
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let opts = [Raw::Tuple(&replace)];
    let changed = frame_with(
        if case == 2 {
            Raw::List(&opts)
        } else {
            Raw::List(&[])
        },
        &orders,
        b"q",
        if case == 3 { 100 } else { 0 },
    )?;
    let encoded = if case == 0 { original } else { &changed };
    let mut charge = PolicySchemaCharge::new(schema);
    let held = prepare_original_policy_set_request(
        &Raw::Binary(b"held"),
        &Raw::List(&[]),
        schema,
        &mut charge,
    )?
    .map_err(|_| "prior charged owner")?;
    assert!(charge.g() > 0 && charge.p() > 0);
    let verified = prepare_original_policy_set_point_frame_bounded(
        encoded,
        schema,
        AdmissionLimits::default(),
        &mut charge,
        workspace(schema, encoded)?,
    )?;
    let before = (charge.g(), charge.p());
    let classification = inspect_original_policy_set_point_frame_bounded(
        PointGateView {
            scope: &namespace,
            control,
            retained_nonempty: true,
            outcome: Some(outcome),
        },
        verified,
        log(u64::from(case != 0)),
        schema,
        AdmissionLimits::default(),
        &mut charge,
        workspace(schema, encoded)?,
    )?;
    let OriginalPolicySetPointInspection::Complete(plan) = classification else {
        return Err("NonFresh reconstructed current/Patch work".into());
    };
    // Independent header/control/request/retained events in gate.rs. Conflict
    // stops before the final128 expiry comparison; no second gate is paid.
    assert_eq!(charge.g() - before.0, [390, 1235, 1107, 1235][case]);
    assert_eq!(charge.p(), before.1);
    assert_eq!(held.workflow_type(), b"held");
    assert_nonfresh_decision(&plan, case, outcome)?;
    assert_full_poison(encoded, control, outcome, case, schema)
}

fn assert_nonfresh_decision(plan: &Plan<'_>, case: usize, outcome: &RetainedOutcome) -> TestResult {
    match (case, plan.decision()) {
        (0, DispatchDecision::LogReplay) => assert!(plan.control().is_log_replay()),
        (1, DispatchDecision::Replay(actual)) => {
            let actual: &RetainedOutcome = actual;
            assert!(std::ptr::eq(actual, outcome));
            assert_eq!(actual.response().as_ptr(), outcome.response().as_ptr());
            assert_eq!(actual.response(), minimal(1)?);
            assert_eq!(
                (actual.original_time().get(), actual.expires_at().get()),
                (0, 100)
            );
        }
        (2, DispatchDecision::IntentConflict) | (3, DispatchDecision::Expired) => {}
        _ => return Err("wrong original retained classification".into()),
    }
    assert!(plan.workflow_type().is_none());
    assert!(plan.used_history_max_events_limit().is_none());
    assert!(plan.used_migration_generations().is_none());
    Ok(())
}

fn assert_full_poison(
    encoded: &[u8],
    control: ControlState,
    outcome: &RetainedOutcome,
    case: usize,
    schema: PolicySchemaLimits,
) -> TestResult {
    let namespace = scope()?;
    let outcomes = BTreeMap::from([(RequestId::new(b"q".to_vec())?, outcome.clone())]);
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Other)]);
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let ranks = [(0, &[99_u32][..], &[99_u32][..])];
    let orders = super::OriginalPolicySetMapOrders::new(
        digest(super::EMPTY_PATCH),
        Some(&[0]),
        &[(0, &[], &[])],
        &ranks,
        &ranks,
    );
    let mut supplied = view(
        GateView {
            scope: &namespace,
            control,
            outcomes: &outcomes,
        },
        &policies,
        &failed,
        &failed,
        OriginalEtfSafety::new(&["x", "x"], &[], None, &[]),
    );
    supplied.history_max_events_limit = Err(ApplyError::InvalidAuthority);
    supplied.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    let mut charge = PolicySchemaCharge::new(schema);
    let plan = decide_original_policy_set_frame_bounded(
        OriginalPolicySetApplyView::new(supplied, &orders),
        encoded,
        log(u64::from(case != 0)),
        schema,
        AdmissionLimits::default(),
        &mut charge,
        workspace(schema, encoded)?,
    )?;
    assert_nonfresh_decision(&plan, case, &outcomes[&RequestId::new(b"q".to_vec())?])
}

#[test]
fn sealed_full_and_point_bind_original_predecessor_and_used_context() -> TestResult {
    let schema = profile(2_000_000_000)?;
    let raw = hex(
        "836803770e666c6f775f706f6c6963795f7631610174000000027704747970656d0000000172770776657273696f6e6e11000100000000000000000000000000000001",
    )?;
    let other = hex(
        "836803770e666c6f775f706f6c6963795f7631610174000000027704747970656d0000000172770776657273696f6e6e11000100000000000000000000000000000002",
    )?;
    for rejected in [false, true] {
        assert_seal(&raw, &other, rejected, schema)?;
    }
    assert_full_portable_merge(schema)?;
    Ok(())
}

fn assert_full_portable_merge(schema: PolicySchemaLimits) -> TestResult {
    let namespace = scope()?;
    let raw = minimal(1)?;
    let encoded = frame(Raw::List(&[]), &empty_orders())?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let migrations = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let ranks = [(0, &[99_u32][..], &[99_u32][..])];
    let orders = super::OriginalPolicySetMapOrders::new(
        digest(super::EMPTY_PATCH),
        Some(&[0]),
        &[(0, &[], &[])],
        &ranks,
        &ranks,
    );
    // Only PATCH facts are committed. This new consumable profile1 route must
    // derive reached current/merged order and fixed safety from actual bytes.
    let supplied = view(
        GateView {
            scope: &namespace,
            control: ControlState::initial(stamp(0)?),
            outcomes: &outcomes,
        },
        &policies,
        &migrations,
        &migrations,
        OriginalEtfSafety::new(&["x", "x"], &[], None, &[]),
    );
    let mut charge = PolicySchemaCharge::new(schema);
    let point = point_plan(
        &encoded,
        &namespace,
        authority(ReadValue::Binary(&raw)),
        schema,
        &mut charge,
    )?;
    let full = decide_original_policy_set_frame_bounded(
        OriginalPolicySetApplyView::new(supplied, &orders),
        &encoded,
        log(0),
        schema,
        AdmissionLimits::default(),
        &mut charge,
        workspace(schema, &encoded)?,
    )?;
    accepted(&full, 1, 2, &minimal(2)?)?;
    assert_eq!(retained(&point)?, retained(&full)?);
    assert_eq!(full.portable_profile_id(), Some(1));
    assert!(full.matches_predecessor(Some(&raw)));
    let mut changed_revision = supplied;
    changed_revision.revision = 7;
    let wrong = decide_original_policy_set_frame_bounded(
        OriginalPolicySetApplyView::new(changed_revision, &orders),
        &encoded,
        log(0),
        schema,
        AdmissionLimits::default(),
        &mut charge,
        workspace(schema, &encoded)?,
    )?;
    assert_eq!(wrong.expected_revision(), 7);
    assert_eq!(
        wf_ports::state::GroupOriginalPolicyChanges::new(
            &namespace,
            supplied.gate.control,
            Some(&raw),
            wrong,
            (AdmissionLimits::default(), schema),
            &mut charge,
            1_000_000,
        )
        .err(),
        Some(wf_ports::state::StateTransactionError::InvalidChanges),
    );
    Ok(())
}

fn assert_seal(raw: &[u8], other: &[u8], rejected: bool, schema: PolicySchemaLimits) -> TestResult {
    let namespace = scope()?;
    let orders = empty_orders();
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let guard = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let opts = [Raw::Tuple(&replace), Raw::Tuple(&guard)];
    let encoded = frame(
        Raw::List(if rejected { &opts } else { &opts[..1] }),
        &orders,
    )?;
    let history = if rejected {
        Err(ApplyError::InvalidAuthority)
    } else {
        Ok(64)
    };
    let current = OriginalPolicySetFreshView {
        current: ReadValue::Binary(raw),
        active_migration: Ok(7),
        completed_migration: Ok(9),
        history_max_events_limit: history,
        outcome_retention_ms: Ok(100),
    };
    let mut charge = PolicySchemaCharge::new(schema);
    let point = point_plan(&encoded, &namespace, current, schema, &mut charge)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(raw))]);
    let active = BTreeMap::from([(b"r".to_vec(), Ok(7))]);
    let completed = BTreeMap::from([(b"r".to_vec(), Ok(9))]);
    let outcomes = BTreeMap::new();
    let mut supplied = view(
        GateView {
            scope: &namespace,
            control: ControlState::initial(stamp(0)?),
            outcomes: &outcomes,
        },
        &policies,
        &active,
        &completed,
        OriginalEtfSafety::new(&["flow_policy_v1", "type", "version"], &[], None, &[]),
    );
    supplied.history_max_events_limit = history;
    let full = decide_original_policy_set_frame_bounded(
        OriginalPolicySetApplyView::new(supplied, &orders),
        &encoded,
        log(0),
        schema,
        AdmissionLimits::default(),
        &mut charge,
        workspace(schema, &encoded)?,
    )?;
    for plan in [&point, &full] {
        assert_eq!(plan.workflow_type(), Some(b"r".as_slice()));
        assert!(plan.matches_predecessor(Some(raw)));
        assert!(!plan.matches_predecessor(Some(other)));
        assert!(!plan.matches_predecessor(None));
        assert_eq!(
            plan.used_history_max_events_limit(),
            (!rejected).then_some(64)
        );
        assert_eq!(
            plan.used_migration_generations(),
            (!rejected).then_some((7, 9))
        );
        assert_eq!(plan.portable_profile_id(), Some(1));
        if rejected {
            assert!(matches!(
                plan.decision(),
                DispatchDecision::Fresh((None, _))
            ));
            assert_eq!(
                retained(plan)?.response(),
                b"ERR stale flow policy generation"
            );
        } else {
            accepted(plan, 1, 10, &minimal(10)?)?;
        }
    }
    assert_eq!(retained(&point)?, retained(&full)?);
    if !rejected {
        let different = OriginalPolicySetFreshView {
            current: ReadValue::Binary(raw),
            active_migration: Ok(7),
            completed_migration: Ok(9),
            history_max_events_limit: Ok(65),
            outcome_retention_ms: Ok(100),
        };
        let alternate = point_plan(&encoded, &namespace, different, schema, &mut charge)?;
        assert_eq!(
            retained(&alternate)?.response(),
            retained(&point)?.response()
        );
        assert_eq!(alternate.used_history_max_events_limit(), Some(65));
        assert_eq!(point.used_history_max_events_limit(), Some(64));
    }
    Ok(())
}

#[test]
fn bounded_header_fresh_and_original_owner_overlap_are_prospective() -> TestResult {
    let schema = profile(2_000_000_000)?;
    let encoded = frame(Raw::List(&[]), &empty_orders())?;
    let pg = usize::try_from(schema.max_work_bytes())?;
    let header = pg + 65_536 + 2048 + 3 * encoded.len();
    let required = header + 2 * 263_168 + 65_535 + 1024;
    let mut charge = PolicySchemaCharge::new(schema);
    assert_eq!(
        prepare_original_policy_set_point_frame_bounded(
            &encoded,
            schema,
            AdmissionLimits::default(),
            &mut charge,
            header - 1,
        )
        .err(),
        Some(RawError::Workspace {
            bytes: header,
            maximum: header - 1
        }),
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let verified = prepare_original_policy_set_point_frame_bounded(
        &encoded,
        schema,
        AdmissionLimits::default(),
        &mut charge,
        header,
    )?;
    assert_eq!(verified.encoded(), encoded);
    drop(verified);
    for maximum in [required - 1, required] {
        let namespace = scope()?;
        let donor = encoded.clone();
        let mut charge = PolicySchemaCharge::new(schema);
        let prepared = fresh(&donor, &namespace, schema, &mut charge, maximum)?;
        let before = (charge.g(), charge.p());
        let result = decide_original_policy_set_point_fresh_bounded(
            prepared,
            authority(ReadValue::Absent),
            schema,
            AdmissionLimits::default(),
            &mut charge,
            maximum,
        );
        if maximum < required {
            assert_eq!(
                result.err(),
                Some(RawError::Workspace {
                    bytes: required,
                    maximum
                })
            );
            assert_eq!((charge.g(), charge.p()), before);
        } else {
            let plan: Plan<'static> = result?;
            drop(namespace);
            drop(donor);
            accepted(&plan, 0, 1, &minimal(1)?)?;
            assert!(plan.matches_predecessor(None));
            assert!(!plan.matches_predecessor(Some(&minimal(0)?)));
        }
    }
    assert_profile_and_malformed_priority(&encoded, schema, header)?;
    Ok(())
}

fn assert_profile_and_malformed_priority(
    encoded: &[u8],
    schema: PolicySchemaLimits,
    header: usize,
) -> TestResult {
    let mut wrong = PolicySchemaCharge::new(profile(2_000_000_001)?);
    assert_eq!(
        prepare_original_policy_set_point_frame_bounded(
            encoded,
            schema,
            AdmissionLimits::default(),
            &mut wrong,
            header,
        )
        .err(),
        Some(RawError::Replace(ExistingError::Request(
            PolicySchemaError::ProfileMismatch.into()
        ))),
    );
    assert_eq!((wrong.g(), wrong.p()), (0, 0));
    let mut malformed = encoded.to_vec();
    malformed.push(0);
    let mut charge = PolicySchemaCharge::new(schema);
    // Header ownership is admitted; the complete grammar/EOF must still fail
    // before an unrelated Fresh output-workspace refusal can mask it.
    let error = prepare_original_policy_set_point_frame_bounded(
        &malformed,
        schema,
        AdmissionLimits::default(),
        &mut charge,
        header + 3,
    )
    .err()
    .ok_or("trailing original frame accepted")?;
    assert!(!matches!(error, RawError::Workspace { .. }));
    assert!(charge.g() > 0);
    Ok(())
}
