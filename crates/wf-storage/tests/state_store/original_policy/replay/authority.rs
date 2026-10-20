//! Consumed original publication repeats actual current and used context.
use super::{TestResult, f, install, public_frame, raw_policy};
use std::path::Path;
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::state::StateTransactionError;

#[test]
fn consumed_original_changes_repeat_all_fresh_authority() -> TestResult {
    use wf_domain::policy::complete::{
        OriginalPolicyReadValue as Read, OriginalPolicySetFreshView as View,
    };
    use wf_domain::workflow::batch::ApplyError;
    use wf_ports::state::GroupOriginalPolicyChanges;
    let dir = f::directory("original-consumed-authority")?;
    let path = dir.join("state");
    drop(f::create(&path, &[], 64)?);
    install(&path, b"job", b"original", 0, 10, 64)?;
    let before = f::cohort(&path)?;
    let s = f::scope()?;
    let caps = f::limits();
    let old = raw_policy(1, b"job")?;
    let different = f::etf_policy(
        1,
        b"job",
        &[(b"states", &f::etf_map(&[])?), (b"version", &[97, 7])],
    )?;
    let history = [(Raw::Atom("history_max_events"), Raw::Integer(65))];
    for case in 0..5 {
        let attrs = if case == 2 {
            vec![(Raw::Atom("retention"), Raw::Map(&history))]
        } else {
            Vec::new()
        };
        let generation = [
            Raw::Atom("expected_generation"),
            Raw::Integer(if case == 4 { 0 } else { 1 }),
        ];
        let frame = public_frame(
            b"job",
            b"attack",
            100,
            Raw::Map(&attrs),
            Raw::List(&[Raw::Tuple(&generation)]),
        )?;
        let current = match case {
            0 => Some(different.as_slice()),
            1 | 4 => None,
            _ => Some(old.as_slice()),
        };
        let view = View {
            current: current.map_or(Read::Absent, Read::Binary),
            active_migration: Ok(if case == 3 { 3 } else { 0 }),
            completed_migration: Ok(0),
            history_max_events_limit: Ok(if case == 2 { 65 } else { 64 }),
            outcome_retention_ms: Ok(1000),
        };
        let mut store = f::open(&path, 64)?;
        let mut charge = PolicySchemaCharge::new(caps.schema);
        let (control, plan) = point_plan(&mut store, &frame, view, &mut charge)?;
        assert_eq!(plan.workflow_type(), Some(b"job".as_slice()));
        assert!(plan.matches_predecessor(current));
        let changes = GroupOriginalPolicyChanges::new(
            &s,
            control,
            current,
            plan,
            (caps.codec.admission(), caps.schema),
            &mut charge,
            1_000_000,
        )?;
        refused(&mut store, changes, &mut charge)?;
        drop(store);
        assert_eq!(f::cohort(&path)?, before);
        drop(f::open(&path, 64)?);
    }
    let poisoned = View {
        current: Read::Binary(&old),
        active_migration: Err(ApplyError::InvalidAuthority),
        completed_migration: Err(ApplyError::InvalidAuthority),
        history_max_events_limit: Err(ApplyError::InvalidAuthority),
        outcome_retention_ms: Ok(1000),
    };
    accepted(&path, poisoned, &old, true, &before)?;
    let good = View {
        current: Read::Binary(&old),
        active_migration: Ok(0),
        completed_migration: Ok(0),
        history_max_events_limit: Ok(64),
        outcome_retention_ms: Ok(1000),
    };
    // The preceding stale rejection advances control only; the same predecessor
    // remains real. An authentic accepted counterpart then publishes gen2.
    let after_stale = f::cohort(&path)?;
    accepted(&path, good, &old, false, &after_stale)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn point_plan(
    store: &mut wf_storage::state::RedbStateStore,
    frame: &[u8],
    view: wf_domain::policy::complete::OriginalPolicySetFreshView<'_>,
    charge: &mut PolicySchemaCharge,
) -> TestResult<(
    wf_domain::workflow::control::ControlState,
    wf_ports::log::OriginalPolicySetRequestPlan<'static>,
)> {
    use wf_domain::workflow::request::gate::PointGateView;
    use wf_ports::log::{
        OriginalPolicySetPointInspection, decide_original_policy_set_point_fresh_bounded,
        inspect_original_policy_set_point_frame_bounded,
        prepare_original_policy_set_point_frame_bounded,
    };
    use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
    let s = f::scope()?;
    let caps = f::limits();
    let verified = prepare_original_policy_set_point_frame_bounded(
        frame,
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let request = verified.header(f::log(1)).request;
    let (control, nonempty) = {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        assert!(tx.outcome(s.tenant(), request)?.is_none());
        (tx.control()?, tx.retained_nonempty(s.tenant())?)
    };
    let at = control.position().get() + 1;
    let inspected = inspect_original_policy_set_point_frame_bounded(
        PointGateView {
            scope: &s,
            control,
            outcome: None,
            retained_nonempty: nonempty,
        },
        verified,
        f::log(at),
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let OriginalPolicySetPointInspection::Fresh(fresh) = inspected else {
        return Err("actual point was not Fresh".into());
    };
    let plan = decide_original_policy_set_point_fresh_bounded(
        *fresh,
        view,
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    Ok((control, plan))
}

fn refused(
    store: &mut wf_storage::state::RedbStateStore,
    changes: wf_ports::state::GroupOriginalPolicyChanges<'_>,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, GroupOriginalPolicyTransaction};
    let s = f::scope()?;
    let caps = f::limits();
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(
        tx.commit_original_policy(changes, caps.schema, charge),
        Err(StateTransactionError::InvalidChanges)
    );
    let healthy = store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(healthy.control()?.position().get(), 0);
    Ok(())
}

fn accepted(
    path: &Path,
    view: wf_domain::policy::complete::OriginalPolicySetFreshView<'_>,
    old: &[u8],
    stale: bool,
    before: &f::Cohort,
) -> TestResult {
    use wf_domain::policy::installation::PolicyError;
    use wf_domain::workflow::request::{DispatchDecision, PolicyRejection};
    use wf_ports::state::{
        GroupApplyStore, GroupOriginalPolicyChanges, GroupOriginalPolicyTransaction,
    };
    let s = f::scope()?;
    let caps = f::limits();
    let request = if stale {
        b"unused-context".as_slice()
    } else {
        b"good"
    };
    let expected = [
        Raw::Atom("expected_generation"),
        Raw::Integer(if stale { 0 } else { 1 }),
    ];
    let bytes = public_frame(
        b"job",
        request,
        100,
        Raw::Map(&[]),
        Raw::List(&[Raw::Tuple(&expected)]),
    )?;
    let mut store = f::open(path, 64)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let (control, plan) = point_plan(&mut store, &bytes, view, &mut charge)?;
    let at = control.position().get() + 1;
    assert_eq!(
        plan.used_history_max_events_limit(),
        if stale { None } else { Some(64) }
    );
    assert_eq!(
        plan.used_migration_generations(),
        if stale { None } else { Some((0, 0)) }
    );
    let (status, body) = if stale {
        (
            RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?),
            b"ERR stale flow policy generation".to_vec(),
        )
    } else {
        (RetainedStatus::Accepted, raw_policy(2, b"job")?)
    };
    assert!(
        matches!(plan.decision(),DispatchDecision::Fresh((_,outcome))if outcome.status()==status&&outcome.response()==body)
    );
    let changes = GroupOriginalPolicyChanges::new(
        &s,
        control,
        Some(old),
        plan,
        (caps.codec.admission(), caps.schema),
        &mut charge,
        1_000_000,
    )?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(
        tx.commit_original_policy(changes, caps.schema, &mut charge)?,
        6
    );
    drop(store);
    let mut expected = before.clone();
    let metadata = f::metadata(&s, request, at, 100, f::frame_intent(&bytes, at)?, status)?;
    f::finish(
        &mut expected,
        &s,
        &bytes,
        f::log(at),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, &body)),
    )?;
    if !stale {
        f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), body);
        f::upgrade6(
            &mut expected,
            &s,
            f::catalog(&s, [None, None], None, None, None)?,
        )?;
    }
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, 64)?);
    Ok(())
}
