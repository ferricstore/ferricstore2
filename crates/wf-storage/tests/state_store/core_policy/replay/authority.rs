//! Actual policy predecessor and immutable-context authority at consumed commit.

use super::{TestResult, f};
use wf_domain::policy::complete::{CompleteCorePolicyPatchFreshView, PolicySchemaCharge};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
use wf_domain::policy::installation::PolicyError;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{
    DispatchDecision, PolicyRejection, RequestScope, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, FreshCompleteCorePolicyPatchFrame,
    decide_complete_core_policy_patch_point_fresh, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyChanges, GroupCorePolicyTransaction,
    StateTransactionError,
};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::RecordBatchTransactionLimits;
use wf_types::identity::RequestId;

#[test]
fn rejected_core_patch_rechecks_actual_policy_predecessor() -> TestResult {
    let dir = f::directory("policy-rejected-predecessor")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let old = f::policy(b"job", 1, &[], false)?;
    let mut store = f::create(&path, &s, &[b"p"], &[(s.tenant().clone(), old)])?;
    drop(store);
    let before = f::cohort(&path)?;
    store = f::open(&path, &s)?;
    let caps = f::limits();
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let request = RequestId::new(b"forged".to_vec())?;
    let (control, nonempty, original, history) = {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        assert_eq!(tx.core_policy_layout()?, 3);
        (
            tx.control()?,
            tx.retained_nonempty(s.tenant())?,
            tx.core_policy_outcome(s.tenant(), &request, caps.schema, &mut charge)?,
            tx.core_policy_history_max_events_limit(),
        )
    };
    let bytes = f::frame(&s, b"forged", 100, b"job", Some(1), PolicyPatch::default())?;
    let fresh = fresh_frame(
        &s,
        bytes.as_bytes(),
        control,
        (nonempty, original.as_ref()),
        caps,
        &mut charge,
    )?;
    // Deliberately false mathematical absence; actual genesis contains job/gen1.
    let plan = decide_complete_core_policy_patch_point_fresh(
        fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"job",
            current: Ok(None),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(history),
            outcome_retention_ms: Ok(caps.outcome_ttl_ms),
        },
        &mut charge,
    )?;
    let changes = GroupCorePolicyChanges::new(
        &s,
        control,
        None,
        plan,
        (caps.schema, caps.codec.admission()),
        &mut charge,
        0,
    )?;
    assert_refused_commit(&mut store, &s, &changes, caps, &mut charge)?;
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    drop(f::open(&path, &s)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn core_patch_preparation_cannot_raise_the_actual_history_limit() -> TestResult {
    let dir = f::directory("policy-history-authority")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let caps = f::limits();
    assert_direct_core_publication_reuses_owner(&dir.join("direct-cache"), &s, caps)?;
    assert_unused_history_is_stale(&dir.join("unused"), &s, caps)?;
    drop(f::create(&path, &s, &[b"p"], &[])?);
    let before = f::cohort(&path)?;
    let mut store = f::open(&path, &s)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let request = RequestId::new(b"history-escape".to_vec())?;
    let (control, nonempty, original, history) = {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        assert!(
            tx.installed_policy(s.tenant(), b"job", caps.schema, &mut charge)?
                .is_none()
        );
        (
            tx.control()?,
            tx.retained_nonempty(s.tenant())?,
            tx.core_policy_outcome(s.tenant(), &request, caps.schema, &mut charge)?,
            tx.core_policy_history_max_events_limit(),
        )
    };
    assert_eq!(history, 64);
    let bytes = f::frame(
        &s,
        b"history-escape",
        100,
        b"job",
        Some(0),
        PolicyPatch {
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(65),
            }),
            ..PolicyPatch::default()
        },
    )?;
    let fresh = fresh_frame(
        &s,
        bytes.as_bytes(),
        control,
        (nonempty, original.as_ref()),
        caps,
        &mut charge,
    )?;
    // True absence, but ordinary supplied history context deliberately raises64 to65.
    let plan = decide_complete_core_policy_patch_point_fresh(
        fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"job",
            current: Ok(None),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(history + 1),
            outcome_retention_ms: Ok(caps.outcome_ttl_ms),
        },
        &mut charge,
    )?;
    let DispatchDecision::Fresh((Some((_, candidate)), outcome)) = plan.decision() else {
        return Err("mathematical history65 fixture did not accept".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(
        candidate
            .data()
            .retention
            .as_ref()
            .and_then(|r| r.history_max_events),
        Some(65)
    );
    let changes = GroupCorePolicyChanges::new(
        &s,
        control,
        None,
        plan,
        (caps.schema, caps.codec.admission()),
        &mut charge,
        1_000_000,
    )?;
    assert_refused_commit(&mut store, &s, &changes, caps, &mut charge)?;
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    drop(f::open(&path, &s)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_unused_history_is_stale(
    path: &std::path::Path,
    s: &RequestScope,
    caps: RecordBatchTransactionLimits,
) -> TestResult {
    let mut store = f::create(path, s, &[b"p"], &[])?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let request = RequestId::new(b"unused-history".to_vec())?;
    let (control, nonempty, original, history) = {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        assert!(
            tx.installed_policy(s.tenant(), b"job", caps.schema, &mut charge)?
                .is_none()
        );
        (
            tx.control()?,
            tx.retained_nonempty(s.tenant())?,
            tx.core_policy_outcome(s.tenant(), &request, caps.schema, &mut charge)?,
            tx.core_policy_history_max_events_limit(),
        )
    };
    assert_eq!(history, 64);
    let bytes = f::frame(
        s,
        b"unused-history",
        100,
        b"job",
        Some(1),
        PolicyPatch::default(),
    )?;
    let fresh = fresh_frame(
        s,
        bytes.as_bytes(),
        control,
        (nonempty, original.as_ref()),
        caps,
        &mut charge,
    )?;
    let plan = decide_complete_core_policy_patch_point_fresh(
        fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"job",
            current: Ok(None),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(history + 1),
            outcome_retention_ms: Ok(caps.outcome_ttl_ms),
        },
        &mut charge,
    )?;
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("unused history changed the stale rejection".into());
    };
    assert_eq!(
        outcome.status(),
        RetainedStatus::PolicyRejected(PolicyRejection::new(PolicyError::StaleGeneration)?)
    );
    assert_eq!(outcome.response(), b"ERR stale flow policy generation");
    let receipt = outcome.clone();
    let next_control = plan.control().next();
    let changes = GroupCorePolicyChanges::new(
        s,
        control,
        None,
        plan,
        (caps.schema, caps.codec.admission()),
        &mut charge,
        0,
    )?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(tx.commit_core_policy(&changes, &mut charge)?, 3);
    drop(store);
    let mut store = f::open(path, s)?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(tx.control()?, next_control);
    assert_eq!(
        tx.core_policy_outcome(s.tenant(), &request, caps.schema, &mut charge)?,
        Some(receipt)
    );
    Ok(())
}

fn assert_direct_core_publication_reuses_owner(
    path: &std::path::Path,
    scope: &RequestScope,
    caps: RecordBatchTransactionLimits,
) -> TestResult {
    drop(f::create(path, scope, &[b"p"], &[])?);
    let before = f::cohort(path)?;
    let mut store = f::open(path, scope)?;
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let control = {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        assert_eq!(tx.core_policy_layout()?, 3);
        assert!(
            tx.installed_policy(scope.tenant(), b"job", caps.schema, &mut charge)?
                .is_none()
        );
        tx.control()?
    };
    let bytes = f::frame(
        scope,
        b"direct",
        100,
        b"job",
        Some(0),
        PolicyPatch::default(),
    )?;
    let fresh = fresh_frame(
        scope,
        bytes.as_bytes(),
        control,
        (false, None),
        caps,
        &mut charge,
    )?;
    let plan = decide_complete_core_policy_patch_point_fresh(
        fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"job",
            current: Ok(None),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(caps.outcome_ttl_ms),
        },
        &mut charge,
    )?;
    let changes = GroupCorePolicyChanges::new(
        scope,
        control,
        None,
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
    assert_eq!(tx.commit_core_policy(&changes, &mut charge)?, 5);
    let (observed_layout, observed_policy) = {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            caps.codec.admission(),
            caps.maximum_read_steps,
            caps.maximum_read_bytes,
            caps.maximum_adapter_bytes,
        )?;
        (
            tx.core_policy_layout(),
            tx.installed_policy(scope.tenant(), b"job", caps.schema, &mut charge),
        )
    };
    drop(store);
    let raw = f::policy_wire(b"job", 1, &[], false)?;
    assert_direct_core_cohort(path, scope, bytes.as_bytes(), &raw, before)?;
    assert_eq!(
        observed_policy?.ok_or("direct policy missing")?.encoded(),
        raw
    );
    assert_eq!(observed_layout, Ok(5));
    Ok(())
}

fn assert_direct_core_cohort(
    path: &std::path::Path,
    scope: &RequestScope,
    frame: &[u8],
    raw: &[u8],
    mut expected: f::Cohort,
) -> TestResult {
    let metadata = f::metadata(
        scope,
        b"direct",
        0,
        100,
        f::frame_intent(frame, 0)?,
        RetainedStatus::Accepted,
    )?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), raw.to_vec());
    f::finish(
        &mut expected,
        scope,
        frame,
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, raw)),
    )?;
    f::upgrade5(
        &mut expected,
        scope,
        f::catalog(scope, [None, None], None, None, None)?,
    )?;
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, scope)?);
    Ok(())
}

fn fresh_frame<'input>(
    scope: &RequestScope,
    bytes: &'input [u8],
    control: ControlState,
    retained: (bool, Option<&RetainedOutcome>),
    caps: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<FreshCompleteCorePolicyPatchFrame<'input>> {
    let verified = prepare_complete_core_policy_patch_point_frame(
        bytes,
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let inspected = inspect_complete_core_policy_patch_point(
        verified,
        f::log(0),
        PointGateView {
            scope,
            control,
            outcome: retained.1,
            retained_nonempty: retained.0,
        },
        (caps.schema, caps.codec.admission()),
        charge,
        caps.workspace_bytes,
    )?;
    let CompleteCorePolicyPatchPointInspection::Fresh(fresh) = inspected else {
        return Err("missing actual Fresh authority fixture".into());
    };
    Ok(*fresh)
}

fn assert_refused_commit(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    changes: &GroupCorePolicyChanges<'_, '_, '_>,
    caps: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?;
    assert_eq!(
        tx.commit_core_policy(changes, charge),
        Err(StateTransactionError::InvalidChanges)
    );
    drop(store.begin_apply(
        scope.group(),
        scope.recovery(),
        caps.codec.admission(),
        caps.maximum_read_steps,
        caps.maximum_read_bytes,
        caps.maximum_adapter_bytes,
    )?);
    Ok(())
}
