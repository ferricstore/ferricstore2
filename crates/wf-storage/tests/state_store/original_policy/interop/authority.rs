//! Sealed actual predecessor and reached immutable context under one writer.
use super::{TestResult, check_disk, f, fifo_patch, original_frame, retained, w};
use std::path::Path;
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::policy::definition::PolicyPatch;
use wf_domain::workflow::request::RequestScope;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::RecordBatchTransactionLimits;

#[test]
fn cross_format_predecessor_history_and_generation_substitution_refuse_atomically() -> TestResult {
    use wf_domain::policy::complete::StoredPolicyValue;
    use wf_ports::state::{
        GroupApplyStore, GroupOriginalPolicyChanges, GroupOriginalPolicyTransaction,
        StateTransactionError,
    };
    let dir = f::directory("interop-authority")?;
    assert_supplied_replay_layouts(&dir)?;
    let path = dir.join("state");
    let scope = f::scope()?;
    let actual = w::rich_value()?;
    let actual_wire = actual.encoded().to_vec();
    let mut store = f::create(&path, &[(b"job".to_vec(), actual_wire.clone())], 64)?;
    let foreign = f::owner(
        &scope,
        b"p",
        f::data(b"foreign", b"other", b"q", 0, Some(0))?,
        0,
        0,
    )?;
    f::seed(&mut store, &scope, &[foreign], 0, 0)?;
    drop(store);
    let before = f::cohort(&path)?;
    let caps = f::limits();
    let history = [(Raw::Atom("history_max_events"), Raw::Integer(65))];
    for case in 0..5 {
        let mut other_generation = actual_wire.clone();
        other_generation[6..14].copy_from_slice(&2_u64.to_be_bytes());
        let wrong_raw = w::rich_wire(1, 7, b"parallel")?;
        let old = match case {
            0 => Some(wrong_raw.as_slice()),
            1 => None,
            4 => Some(other_generation.as_slice()),
            _ => Some(actual_wire.as_slice()),
        };
        let attrs = if case == 2 {
            vec![(Raw::Atom("retention"), Raw::Map(&history))]
        } else {
            Vec::new()
        };
        let expected = [Raw::Atom("expected_generation"), Raw::Integer(1)];
        let bytes = original_frame(
            b"attack",
            10,
            Raw::Map(&attrs),
            Raw::List(&[Raw::Tuple(&expected)]),
        )?;
        store = f::open(&path, 64)?;
        let mut charge = PolicySchemaCharge::new(caps.schema);
        let (control, plan) = original_plan(&mut store, &bytes, old, case, &mut charge)?;
        assert_eq!(plan.workflow_type(), Some(b"job".as_slice()));
        assert!(plan.matches_predecessor(old));
        let changes = GroupOriginalPolicyChanges::new(
            &scope,
            control,
            old,
            plan,
            (caps.codec.admission(), caps.schema),
            &mut charge,
            1_000_000,
        )?;
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        assert_eq!(
            tx.commit_original_policy(changes, caps.schema, &mut charge),
            Err(StateTransactionError::InvalidChanges)
        );
        drop(store);
        check_disk(&path, &before)?;
    }
    let wrong = StoredPolicyValue::Original(w::rich_wire(1, 7, b"parallel")?);
    core_substitution(&path, &before, &wrong, 64, 0)?;
    let actual = StoredPolicyValue::Complete(Box::new(actual));
    core_substitution(&path, &before, &actual, 65, 0)?;
    core_substitution(&path, &before, &actual, 64, 1)?;
    stale_cas(&path, before, &history, &scope, caps)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
fn stale_cas(
    path: &Path,
    before: f::Cohort,
    history: &[(Raw<'_>, Raw<'_>)],
    scope: &RequestScope,
    caps: RecordBatchTransactionLimits,
) -> TestResult {
    // Authentic stale CAS retains a business rejection before the later
    // invalid retention request could mask its generation diagnosis.
    let expected = [Raw::Atom("expected_generation"), Raw::Integer(0)];
    let bytes = original_frame(
        b"stale",
        10,
        Raw::Map(&[(Raw::Atom("retention"), Raw::Map(history))]),
        Raw::List(&[Raw::Tuple(&expected)]),
    )?;
    let mut store = f::open(path, 64)?;
    let outcome = retained(f::apply(
        &mut store,
        &bytes,
        1,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?)?;
    assert_eq!(outcome.response(), b"ERR stale flow policy generation");
    drop(store);
    let mut after = before;
    let metadata = f::metadata(
        scope,
        b"stale",
        1,
        10,
        f::frame_intent(&bytes, 1)?,
        outcome.status(),
    )?;
    f::finish(
        &mut after,
        scope,
        &bytes,
        f::log(1),
        (f::clock(10)?, f::clock(10)?),
        Some((&metadata, 1, b"ERR stale flow policy generation")),
    )?;
    check_disk(path, &after)
}
fn original_plan(
    store: &mut RedbStateStore,
    bytes: &[u8],
    old: Option<&[u8]>,
    case: u8,
    charge: &mut PolicySchemaCharge,
) -> TestResult<(
    wf_domain::workflow::control::ControlState,
    wf_ports::log::OriginalPolicySetRequestPlan<'static>,
)> {
    use wf_domain::policy::complete::{
        OriginalPolicyReadValue as Read, OriginalPolicySetFreshView,
    };
    use wf_domain::workflow::request::gate::PointGateView;
    use wf_ports::log::{
        OriginalPolicySetPointInspection, decide_original_policy_set_point_fresh_bounded,
        inspect_original_policy_set_point_frame_bounded,
        prepare_original_policy_set_point_frame_bounded,
    };
    use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
    let caps = f::limits();
    let scope = f::scope()?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    let control = tx.control()?;
    drop(tx);
    let verified = prepare_original_policy_set_point_frame_bounded(
        bytes,
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let OriginalPolicySetPointInspection::Fresh(fresh) =
        inspect_original_policy_set_point_frame_bounded(
            PointGateView {
                scope: &scope,
                control,
                retained_nonempty: true,
                outcome: None,
            },
            verified,
            f::log(1),
            caps.schema,
            caps.codec.admission(),
            charge,
            caps.workspace_bytes,
        )?
    else {
        return Err("authority fixture was not Fresh".into());
    };
    let plan = decide_original_policy_set_point_fresh_bounded(
        *fresh,
        OriginalPolicySetFreshView {
            current: old.map_or(Read::Absent, Read::Binary),
            active_migration: Ok(if case == 3 { 3 } else { 0 }),
            completed_migration: Ok(0),
            history_max_events_limit: Ok(if case == 2 { 65 } else { 64 }),
            outcome_retention_ms: Ok(1000),
        },
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    Ok((control, plan))
}
fn core_substitution(
    path: &Path,
    before: &f::Cohort,
    old: &wf_domain::policy::complete::StoredPolicyValue,
    history: u32,
    high: u64,
) -> TestResult {
    use wf_domain::policy::complete::CompleteCorePolicyPatchFreshView;
    use wf_domain::policy::definition::FieldPatch;
    use wf_domain::retention::RetentionOverride;
    use wf_domain::workflow::request::gate::PointGateView;
    use wf_ports::log::{
        CompleteCorePolicyPatchPointInspection, decide_complete_core_policy_patch_point_fresh,
        inspect_complete_core_policy_patch_point, prepare_complete_core_policy_patch_point_frame,
    };
    use wf_ports::state::{
        GroupApplyStore, GroupApplyTransaction, GroupCorePolicyChanges, GroupCorePolicyTransaction,
        StateTransactionError,
    };
    let scope = f::scope()?;
    let caps = f::limits();
    let mut store = f::open(path, 64)?;
    let delta = if history == 65 {
        PolicyPatch {
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(65),
            }),
            ..PolicyPatch::default()
        }
    } else {
        fifo_patch()
    };
    let bytes = w::core_frame(b"core-attack", 10, Some(1), delta)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    let control = tx.control()?;
    drop(tx);
    let verified = prepare_complete_core_policy_patch_point_frame(
        &bytes,
        caps.schema,
        caps.codec.admission(),
        &mut charge,
        caps.workspace_bytes,
    )?;
    let CompleteCorePolicyPatchPointInspection::Fresh(fresh) =
        inspect_complete_core_policy_patch_point(
            verified,
            f::log(1),
            PointGateView {
                scope: &scope,
                control,
                retained_nonempty: true,
                outcome: None,
            },
            (caps.schema, caps.codec.admission()),
            &mut charge,
            caps.workspace_bytes,
        )?
    else {
        return Err("Core authority fixture was not Fresh".into());
    };
    let plan = decide_complete_core_policy_patch_point_fresh(
        *fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"job",
            current: Ok(Some(old)),
            active_migration: Ok((high != 0).then_some(high)),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(history),
            outcome_retention_ms: Ok(1000),
        },
        &mut charge,
    )?;
    assert!(plan.matches_predecessor(Some(old)));
    let changes = GroupCorePolicyChanges::new(
        &scope,
        control,
        Some(old),
        plan,
        (caps.schema, caps.codec.admission()),
        &mut charge,
        1_000_000,
    )?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(
        tx.commit_core_policy(&changes, &mut charge),
        Err(StateTransactionError::InvalidChanges)
    );
    drop(store);
    check_disk(path, before)
}

fn assert_supplied_replay_layouts(dir: &Path) -> TestResult {
    use wf_domain::workflow::request::RetainedStatus;
    use wf_storage::transaction::apply_complete_core_policy_patch_transaction;

    let caps = f::limits();
    let scope = f::scope()?;
    let donor_path = dir.join("replay-layout6");
    let value = w::rich_value()?;
    let mut store = f::create(
        &donor_path,
        &[(b"job".to_vec(), value.encoded().to_vec())],
        64,
    )?;
    let set = original_frame(b"layout-set", 0, Raw::Map(&[]), Raw::List(&[]))?;
    let set_outcome = retained(f::apply(
        &mut store,
        &set,
        0,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?)?;
    assert_eq!(set_outcome.response(), w::rich_wire(2, 7, b"parallel")?);
    let first = w::core_frame(b"layout-replay", 11, Some(2), fifo_patch())?;
    let donor = retained(apply_complete_core_policy_patch_transaction(
        &mut store,
        &scope,
        &first,
        f::log(1),
        caps,
    )?)?;
    assert_eq!(
        (
            donor.intent().version(),
            donor.response_version(),
            donor.status()
        ),
        (2, 2, RetainedStatus::Accepted)
    );
    assert_eq!(donor.response(), w::rich_wire(3, 7, b"fifo")?);
    drop(store);
    let incoming = w::core_frame(b"layout-replay", 12, Some(2), fifo_patch())?;
    assert_eq!(donor.intent(), w::core_intent(&incoming, 2)?);
    // The same genuine accepted receipt must work on actual6, while supplied
    // mathematical metadata cannot grant its representation to an old owner.
    assert_supplied_replay(&donor_path, 6, &incoming, &donor)?;
    for layout in [3, 5] {
        let path = dir.join(format!("replay-layout{layout}"));
        let mut store = f::create(&path, &[(b"job".to_vec(), value.encoded().to_vec())], 64)?;
        if layout == 5 {
            let frame = w::core_frame(b"layout-v2", 0, Some(1), PolicyPatch::default())?;
            let outcome = retained(apply_complete_core_policy_patch_transaction(
                &mut store,
                &scope,
                &frame,
                f::log(0),
                caps,
            )?)?;
            assert_eq!(
                (outcome.response_version(), outcome.status()),
                (1, RetainedStatus::Accepted)
            );
        } else {
            let owner = f::owner(
                &scope,
                b"p",
                f::data(b"a", b"other", b"q", 0, Some(0))?,
                0,
                0,
            )?;
            f::seed(&mut store, &scope, &[owner], 0, 0)?;
        }
        let owner = f::owner(
            &scope,
            b"p",
            f::data(b"b", b"other", b"q", 11, Some(11))?,
            0,
            1_u128 << 32,
        )?;
        f::seed(&mut store, &scope, &[owner], 1, 11)?;
        drop(store);
        assert_supplied_replay(&path, layout, &incoming, &donor)?;
    }
    Ok(())
}

fn assert_supplied_replay(
    path: &Path,
    layout: u16,
    incoming: &[u8],
    donor: &wf_domain::workflow::request::RetainedOutcome,
) -> TestResult {
    use wf_domain::workflow::request::{DispatchDecision, gate::PointGateView};
    use wf_ports::log::{
        CompleteCorePolicyPatchPointInspection, inspect_complete_core_policy_patch_point,
        prepare_complete_core_policy_patch_point_frame,
    };
    use wf_ports::state::{
        GroupApplyStore, GroupApplyTransaction, GroupCorePolicyChanges, GroupCorePolicyTransaction,
        StateTransactionError,
    };

    let caps = f::limits();
    let scope = f::scope()?;
    let mut expected = f::cohort(path)?;
    let mut store = f::open(path, 64)?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(tx.core_policy_layout()?, layout);
    let control = tx.control()?;
    assert_eq!(control.position().get(), 1);
    assert_eq!(control.time().hlc().physical().get(), 11);
    drop(tx);
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let verified = prepare_complete_core_policy_patch_point_frame(
        incoming,
        caps.schema,
        caps.codec.admission(),
        &mut charge,
        caps.workspace_bytes,
    )?;
    let CompleteCorePolicyPatchPointInspection::Complete(plan) =
        inspect_complete_core_policy_patch_point(
            verified,
            f::log(2),
            PointGateView {
                scope: &scope,
                control,
                retained_nonempty: true,
                outcome: Some(donor),
            },
            (caps.schema, caps.codec.admission()),
            &mut charge,
            caps.workspace_bytes,
        )?
    else {
        return Err("supplied replay unexpectedly requested Fresh authority".into());
    };
    let DispatchDecision::Replay(original) = plan.decision() else {
        return Err("supplied receipt did not classify Replay".into());
    };
    assert!(std::ptr::eq(*original, donor));
    assert!(plan.workflow_type().is_none());
    let changes = GroupCorePolicyChanges::new(
        &scope,
        control,
        None,
        *plan,
        (caps.schema, caps.codec.admission()),
        &mut charge,
        1_000_000,
    )?;
    let prior = (charge.g(), charge.p());
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    let result = tx.commit_core_policy(&changes, &mut charge);
    if layout == 6 {
        assert_eq!(result?, 6);
        f::finish(
            &mut expected,
            &scope,
            incoming,
            f::log(2),
            (f::clock(12)?, f::clock(12)?),
            None,
        )?;
    } else {
        assert_eq!(result, Err(StateTransactionError::InvalidChanges));
        assert_eq!((charge.g(), charge.p()), prior);
    }
    drop(store);
    check_disk(path, &expected)
}
