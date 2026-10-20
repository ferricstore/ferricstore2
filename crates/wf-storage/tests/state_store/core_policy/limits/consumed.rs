//! Exact consumed C/End prefix; no driver counters or physical-call calibration.

use super::*;
use wf_domain::policy::complete::{CompleteCorePolicyPatchFreshView, StoredPolicyValue};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, CompleteCorePolicyPatchRequestPlan,
    decide_complete_core_policy_patch_point_fresh, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::GroupCorePolicyChanges;

pub(super) fn assert_prefix(dir: &Path) -> TestResult {
    let path = dir.join("consumed-prefix");
    let before = live5_fixture(&path)?;
    let s = f::scope()?;
    let frame = f::frame(
        &s,
        b"q",
        25,
        b"job",
        Some(1),
        f::mode_patch(&[(b"q", Some(StateMode::Parallel))]),
    )?;
    // Independent helper-ordered ledger; see DOMAIN TESTS1 consumed-prefix table.
    let prefix_c: usize = [
        13, 13_280, 536, 0, 0, 661, 512, 465, 13, 1283, 39, 1027, 0, 1101, 39, 86, 44,
    ]
    .into_iter()
    .sum();
    let end_steps: usize = [1, 0, 1, 2, 3, 8, 0, 3, 1, 3, 2, 0, 1, 2, 2, 3, 3]
        .into_iter()
        .sum();
    let validation = 2048 + 216 * 52;
    let encoded = 102 + 1 + 1 + 1 + 1 + 52;
    let exact_c = prefix_c + validation + encoded;
    assert_eq!((prefix_c, end_steps, exact_c), (19_099, 35, 32_537));
    let decode_g = 948 + 1446 + 1033 + 180;
    assert_eq!(decode_g, 3607);
    let ample = f::limits();
    for (low, high) in [
        (
            (ample.maximum_read_steps, exact_c - 1),
            (ample.maximum_read_steps, exact_c),
        ),
        (
            (end_steps - 1, ample.maximum_adapter_bytes),
            (end_steps, ample.maximum_adapter_bytes),
        ),
    ] {
        let lo = commit_prefix(&path, frame.as_bytes(), low, false)?;
        let hi = commit_prefix(&path, frame.as_bytes(), high, false)?;
        assert!(lo.0 > 0, "pre-mutation validation did not run");
        assert_eq!(hi, (lo.0 + decode_g, 0));
        assert_eq!(lo.1, 0);
        assert_eq!(f::cohort(&path)?, before);
    }
    commit_prefix(
        &path,
        frame.as_bytes(),
        (ample.maximum_read_steps, ample.maximum_adapter_bytes),
        true,
    )?;
    assert_eq!(
        f::cohort(&path)?,
        expected_parallel(&before, frame.as_bytes())?
    );
    drop(f::open(&path, &s)?);
    Ok(())
}

fn live5_fixture(path: &Path) -> TestResult<f::Cohort> {
    let s = f::scope()?;
    let mut store = f::create(path, &s, &[b"p"], &[])?;
    let frame = f::frame(
        &s,
        b"initial",
        0,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } =
        apply_complete_core_policy_patch_transaction(
            &mut store,
            &s,
            frame.as_bytes(),
            f::log(0),
            f::limits(),
        )?
    else {
        return Err("real live5 fixture publication did not retain".into());
    };
    assert_eq!(
        (outcome.status(), outcome.response()),
        (RetainedStatus::Accepted, response.as_slice())
    );
    let owner = f::owner(
        &s,
        b"p",
        f::data(b"i", b"job", b"q", 0, Some(0))?,
        0,
        1_u128 << 32,
    )?;
    f::seed(&mut store, &s, std::slice::from_ref(&owner), 1, 0)?;
    drop(store);
    let before = f::cohort(path)?;
    let rows = f::rows(&before)?;
    assert_eq!(
        rows.get(&f::point(&s, &owner)),
        Some(&f::record_wire(&s, &owner)?)
    );
    assert_eq!(rows.get(&f::member(&s, &owner)), Some(&Vec::new()));
    assert_eq!(rows.get(&f::due(&s, &owner)?), Some(&vec![1]));
    assert_eq!(
        rows.get(&f::key(6, &[b"g", b"z", b"t", b"job"])),
        Some(&response)
    );
    let (history_key, history) = f::history_wire(&s, &owner, &f::history(&owner))?;
    assert_eq!(rows.get(&history_key), Some(&history));
    assert_eq!(
        (
            f::point(&s, &owner).len(),
            f::record_wire(&s, &owner)?.len(),
            f::member(&s, &owner).len(),
            f::due(&s, &owner)?.len()
        ),
        (22, 131, 47, 39)
    );
    assert_eq!(response.len(), 52);
    assert_eq!(
        &before[0].as_ref().ok_or("missing live metadata")?[b"manifest".as_slice()][..6],
        b"FWSS\0\x05"
    );
    drop(f::open(path, &s)?);
    Ok(before)
}

fn prepare_plan<'retained>(
    current: ControlState,
    scope: &RequestScope,
    frame: &[u8],
    old: &StoredPolicyValue,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompleteCorePolicyPatchRequestPlan<'retained>> {
    let caps = f::limits();
    let profiles = (caps.schema, caps.codec.admission());
    let verified = prepare_complete_core_policy_patch_point_frame(
        frame,
        profiles.0,
        profiles.1,
        charge,
        caps.workspace_bytes,
    )?;
    let inspected = inspect_complete_core_policy_patch_point(
        verified,
        f::log(2),
        PointGateView {
            scope,
            control: current,
            retained_nonempty: true,
            outcome: None,
        },
        profiles,
        charge,
        caps.workspace_bytes,
    )?;
    let CompleteCorePolicyPatchPointInspection::Fresh(fresh) = inspected else {
        return Err("new consumed fixture request was not Fresh".into());
    };
    Ok(decide_complete_core_policy_patch_point_fresh(
        *fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: b"job",
            current: Ok(Some(old)),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(64),
            outcome_retention_ms: Ok(1000),
        },
        charge,
    )?)
}

fn commit_prefix(
    path: &Path,
    frame: &[u8],
    caps: (usize, usize),
    accept: bool,
) -> TestResult<(u64, u64)> {
    let s = f::scope()?;
    let before = f::cohort(path)?;
    let mut store = f::open(path, &s)?;
    let ample = f::limits();
    // Caller preparation is outside the fresh consumed C/step frontier.
    // Its ONE schema account continues unchanged through constructor and commit.
    let current = {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            ample.codec.admission(),
            ample.maximum_read_steps,
            ample.maximum_read_bytes,
            ample.maximum_adapter_bytes,
        )?;
        tx.control()?
    };
    let old = f::policy(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    let old = StoredPolicyValue::Complete(Box::new(old));
    let mut charge = PolicySchemaCharge::new(ample.schema);
    let plan = prepare_plan(current, &s, frame, &old, &mut charge)?;
    let changes = GroupCorePolicyChanges::new(
        &s,
        current,
        Some(&old),
        plan,
        (ample.schema, ample.codec.admission()),
        &mut charge,
        2048 + 216 * 52,
    )?;
    let prior_g = charge.g();
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        ample.codec.admission(),
        caps.0,
        ample.maximum_read_bytes,
        caps.1,
    )?;
    let result = tx.commit_core_policy(&changes, &mut charge);
    if accept {
        assert_eq!(result?, 5);
    } else {
        assert_eq!(result, Err(StateTransactionError::Limit));
    }
    let delta = charge
        .g()
        .checked_sub(prior_g)
        .ok_or("schema work rolled back")?;
    drop(store.begin_apply(
        s.group(),
        s.recovery(),
        ample.codec.admission(),
        100,
        ample.maximum_read_bytes,
        ample.maximum_adapter_bytes,
    )?);
    drop(store);
    if !accept {
        assert_eq!(f::cohort(path)?, before);
    }
    drop(f::open(path, &s)?);
    Ok((delta, charge.p()))
}

fn expected_parallel(before: &f::Cohort, frame: &[u8]) -> TestResult<f::Cohort> {
    let s = f::scope()?;
    let mut expected = before.clone();
    let owner = f::owner(
        &s,
        b"p",
        f::data(b"i", b"job", b"q", 0, Some(0))?,
        0,
        1_u128 << 32,
    )?;
    let response = f::policy_wire(b"job", 2, &[(b"q", StateMode::Parallel)], false)?;
    let rows = f::rows_mut(&mut expected)?;
    assert_eq!(rows.remove(&f::member(&s, &owner)), Some(Vec::new()));
    assert_eq!(rows.insert(f::due(&s, &owner)?, vec![0]), Some(vec![1]));
    rows.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"q",
        2,
        25,
        f::frame_intent(frame, 2)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        frame,
        f::log(2),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    Ok(expected)
}
