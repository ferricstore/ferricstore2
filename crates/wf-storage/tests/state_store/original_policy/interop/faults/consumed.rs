//! Independent Core/ETF canonical owner and true-End event ledger.
use super::{TestResult, f, initialized, policy};
use std::path::Path;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchFreshView, PolicySchemaCharge, StoredPolicyValue,
};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, CompleteCorePolicyPatchRequestPlan,
    decide_complete_core_policy_patch_point_fresh, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyChanges, GroupCorePolicyTransaction,
    StateTransactionError,
};

pub(super) fn assert_prefix(dir: &Path) -> TestResult {
    let path = dir.join("exact-consumed");
    let (before, expected, bytes, _) = initialized(&path)?;
    let ample = f::limits();
    let x_old = 65_536 + 256 * 80;
    let x_new = 65_536 + 256 * 76;
    // Matcher +four measure/project X owners; actual current's X is read-R.
    // No indexed-effect decoder belongs to the restricted Core Patch arm.
    let before_rows = [
        13,
        x_new,
        536,
        x_old,
        x_old + 1288,
        x_new,
        x_new + 1288,
        0,
        0,
        689,
        512,
    ];
    let first = [465, 13, 569, 1026, 562, 39, 86];
    let second = [477, 13, 0, 0, 562, 39, 86];
    let prefix = before_rows
        .into_iter()
        .chain(first)
        .chain(second)
        .sum::<usize>()
        + 44;
    let end = [
        1, 0, 1, 0, 0, 0, 0, 2, 3, 8, 0, 3, 1, 2, 0, 1, 2, 3, 3, 1, 0, 0, 1, 2, 3, 3,
    ]
    .into_iter()
    .sum::<usize>();
    let read = 512 + 592 + x_old + 277 + 269 + 2 * (278 + 519 + 513);
    let wire = 102 + 1 + 1 + 1 + 7 + 76;
    let through_encoding = prefix + x_new + wire;
    assert_eq!(
        (prefix, end, read, wire, through_encoding),
        (435_315, 40, 90_286, 188, 520_495)
    );
    let decoder = 130 + 8777 + 2 * 31_123 + 784 + 1172 + 540 + 4803 + 796 + 128;
    assert_eq!(decoder, 79_376);
    for (low, high) in [
        (
            (
                ample.maximum_read_steps,
                ample.maximum_read_bytes,
                through_encoding - 1,
            ),
            (
                ample.maximum_read_steps,
                ample.maximum_read_bytes,
                through_encoding,
            ),
        ),
        (
            (
                end - 1,
                ample.maximum_read_bytes,
                ample.maximum_adapter_bytes,
            ),
            (end, ample.maximum_read_bytes, ample.maximum_adapter_bytes),
        ),
        (
            (
                ample.maximum_read_steps,
                read - 1,
                ample.maximum_adapter_bytes,
            ),
            (ample.maximum_read_steps, read, ample.maximum_adapter_bytes),
        ),
    ] {
        let lo = commit_prefix(&path, &bytes, low, false)?;
        let hi = commit_prefix(&path, &bytes, high, false)?;
        assert!(lo.0 > 0 && lo.1 >= 1024);
        assert_eq!(hi, (lo.0 + decoder, lo.1));
        assert_eq!(f::cohort(&path)?, before);
    }
    commit_prefix(
        &path,
        &bytes,
        (
            ample.maximum_read_steps,
            ample.maximum_read_bytes,
            ample.maximum_adapter_bytes,
        ),
        true,
    )?;
    assert_eq!(f::cohort(&path)?, expected);
    drop(f::open(&path, 64)?);
    Ok(())
}
fn prepare_plan(
    control: ControlState,
    bytes: &[u8],
    old: &StoredPolicyValue,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompleteCorePolicyPatchRequestPlan<'static>> {
    let scope = f::scope()?;
    let caps = f::limits();
    let verified = prepare_complete_core_policy_patch_point_frame(
        bytes,
        caps.schema,
        caps.codec.admission(),
        charge,
        caps.workspace_bytes,
    )?;
    let CompleteCorePolicyPatchPointInspection::Fresh(fresh) =
        inspect_complete_core_policy_patch_point(
            verified,
            f::log(2),
            PointGateView {
                scope: &scope,
                control,
                retained_nonempty: true,
                outcome: None,
            },
            (caps.schema, caps.codec.admission()),
            charge,
            caps.workspace_bytes,
        )?
    else {
        return Err("new consumed Core/ETF request was not Fresh".into());
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
    bytes: &[u8],
    caps: (usize, usize, usize),
    accept: bool,
) -> TestResult<(u64, u64)> {
    let scope = f::scope()?;
    let ample = f::limits();
    let before = f::cohort(path)?;
    let mut store = f::open(path, 64)?;
    let current = {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            ample.codec.admission(),
            ample.maximum_read_steps,
            ample.maximum_read_bytes,
            ample.maximum_adapter_bytes,
        )?;
        tx.control()?
    };
    let old = StoredPolicyValue::Original(policy(1, b"parallel")?);
    let mut charge = PolicySchemaCharge::new(ample.schema);
    let plan = prepare_plan(current, bytes, &old, &mut charge)?;
    let changes = GroupCorePolicyChanges::new(
        &scope,
        current,
        Some(&old),
        plan,
        (ample.schema, ample.codec.admission()),
        &mut charge,
        65_536 + 256 * 80,
    )?;
    let prior = charge.g();
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        ample.codec.admission(),
        caps.0,
        caps.1,
        caps.2,
    )?;
    let result = tx.commit_core_policy(&changes, &mut charge);
    if accept {
        assert_eq!(result?, 6);
    } else {
        assert_eq!(result, Err(StateTransactionError::Limit));
    }
    let delta = charge
        .g()
        .checked_sub(prior)
        .ok_or("operation reset its schema work")?;
    let peak = charge.p();
    drop(store.begin_apply(
        scope.group(),
        scope.recovery(),
        ample.codec.admission(),
        ample.maximum_read_steps,
        ample.maximum_read_bytes,
        ample.maximum_adapter_bytes,
    )?);
    drop(store);
    if !accept {
        assert_eq!(f::cohort(path)?, before);
    }
    drop(f::open(path, 64)?);
    Ok((delta, peak))
}
