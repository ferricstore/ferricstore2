//! Actual eight frames and original retained outcomes through startup recovery.

use std::path::Path;

use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::{Operation, StartAndClaim};
use wf_ports::blob::{OriginalValueRefKind, WorkflowValueReadBatch};
use wf_ports::consensus::{WorkflowValuePolicyWitness, WorkflowValueTargetGuard};
use wf_ports::log::{PreparedBytes, RaftLogStore};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::RecordBatchTransactionOutcome as Outcome;

use super::{TestResult, f, fixtures as v, recover_committed};

#[test]
fn value_recovery_replays_all_contained_modes_and_preserves_births() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    for original in [false, true] {
        for mode in [1, 5, 6, 7] {
            mode_case(mode, original, &limits)?;
        }
    }
    Ok(())
}

fn mode_case(mode: u8, original: bool, limits: &super::Limits) -> TestResult {
    let directory = f::directory("node-value-mode")?;
    let reference = directory.join("reference");
    let live = directory.join("live");
    std::fs::create_dir(&reference)?;
    std::fs::create_dir(&live)?;
    let mut expected = None;
    for (path, direct) in [(&reference, true), (&live, false)] {
        let (mut log, mut state) = v::pair(path)?;
        let policy = v::install(&mut log, &mut state, original, None, 0, 10)?;
        let frame = mode_frame(&mut log, &mut state, mode, &policy)?;
        let at = if mode == 1 { 2 } else { 1 };
        let now = if mode == 1 { 200 } else { 100 };
        f::append(&mut log, at, frame.as_bytes())?;
        log.save_committed(f::at(at))?;
        if direct {
            v::accepted(v::apply(&mut state, &frame, at)?, at, now, mode)?;
            drop((log, state));
            expected = Some(v::cohort(path)?);
        } else {
            drop((log, state));
            let scope = f::scope()?;
            let (log, state) = f::open_pair(path)?;
            let (log, mut state) =
                recover_committed(log, state, scope.group(), scope.recovery(), *limits)?;
            let outcome = v::outcome(&mut state, b"mode")?;
            assert_eq!(outcome.intent().version(), 8);
            assert_eq!(
                outcome.response_version(),
                if mode == 1 { 8 } else { u16::from(mode) }
            );
            assert_eq!(outcome.original_position(), f::at(at));
            assert_eq!(outcome.original_time().get(), now);
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
            assert_eq!(
                v::control(&mut state)?
                    .last_entry()
                    .map(wf_domain::workflow::control::EntryIdentity::log),
                Some(f::at(at))
            );
            let read = verify_mode(&state, mode, &policy)?;
            drop((frame, outcome, state, log));
            if mode == 1 {
                v::assert_read(
                    &read,
                    &[b"value", b"cancelled"],
                    &[Some(0), Some(1), Some(0)],
                );
            } else {
                v::assert_read(&read, &[b"value"], &[Some(0), Some(0)]);
            }
            assert_eq!(
                &v::cohort(path)?,
                expected.as_ref().ok_or("reference absent")?
            );
            for _ in 0..2 {
                drop(f::open_pair(path)?);
                assert_eq!(
                    &v::cohort(path)?,
                    expected.as_ref().ok_or("reference absent")?
                );
            }
        }
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

pub(super) fn mode_frame(
    log: &mut wf_storage::log::RedbLogStore,
    state: &mut RedbStateStore,
    mode: u8,
    policy: &WorkflowValuePolicyWitness,
) -> TestResult<PreparedBytes> {
    let scope = f::scope()?;
    let (now, item) = if mode == 1 {
        let base = v::batch(
            &scope,
            5,
            b"base",
            100,
            vec![v::create(b"i", 100, b"value")?],
            v::copy_policy(policy),
        )?;
        let frame = v::encode(&base)?;
        f::append(log, 1, frame.as_bytes())?;
        v::accepted(v::apply(state, &frame, 1)?, 1, 100, 5)?;
        (200, v::item(b"i", 200, f::cancel_op())?)
    } else if mode == 7 {
        (
            100,
            v::item(
                b"i",
                100,
                Operation::StartAndClaim(StartAndClaim {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    worker: b"worker".to_vec(),
                    lease_ms: 5_000,
                    payload: Some(b"value".to_vec()),
                }),
            )?,
        )
    } else {
        (100, v::create(b"i", 100, b"value")?)
    };
    v::encode(&v::batch(
        &scope,
        mode,
        b"mode",
        now,
        vec![item],
        v::copy_policy(policy),
    )?)
}

pub(super) fn verify_mode(
    state: &RedbStateStore,
    mode: u8,
    policy: &WorkflowValuePolicyWitness,
) -> TestResult<WorkflowValueReadBatch> {
    let current = if mode == 1 {
        [Some(1), None, Some(2)]
    } else {
        [Some(1), None, None]
    };
    v::binding(
        state,
        b"i",
        None,
        (1_u128 << 32, current, (mode == 1).then_some(22_200)),
        policy,
    )?;
    v::binding(
        state,
        b"i",
        Some("100-1"),
        (1_u128 << 32, [Some(1), None, None], None),
        policy,
    )?;
    let payload = v::reference(b"i", OriginalValueRefKind::Payload, 1)?;
    if mode == 1 {
        v::binding(
            state,
            b"i",
            Some("200-2"),
            (1_u128 << 32, current, Some(22_200)),
            policy,
        )?;
        let error = v::reference(b"i", OriginalValueRefKind::Error, 2)?;
        v::read(state, &[&payload, &error, &payload])
    } else {
        v::read(state, &[&payload, &payload])
    }
}

#[test]
fn value_recovery_retained_reprepare_preserves_original_outcome() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    let reference = f::directory("value-retained-reference")?;
    let live = f::directory("value-retained-live")?;
    retained_case(&reference, true, &limits)?;
    retained_case(&live, false, &limits)?;
    assert_eq!(v::cohort(&live)?, v::cohort(&reference)?);
    assert_eq!(f::log_rows(&live)?, f::log_rows(&reference)?);
    assert_eq!(f::log_meta(&live)?, f::log_meta(&reference)?);
    drop(f::open_pair(&live)?);
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(live)?;
    Ok(())
}

fn retained_case(path: &Path, direct: bool, limits: &super::Limits) -> TestResult {
    let (mut log, mut state) = v::pair(path)?;
    let first = v::install(&mut log, &mut state, false, None, 0, 10)?;
    let scope = f::scope()?;
    let birth = v::encode(&v::batch(
        &scope,
        5,
        b"same",
        20,
        vec![v::create(b"birth", 20, b"A")?],
        first,
    )?)?;
    f::append(&mut log, 1, birth.as_bytes())?;
    v::accepted(v::apply(&mut state, &birth, 1)?, 1, 20, 5)?;
    let updated = v::install(&mut log, &mut state, false, Some(1), 2, 30)?;
    for (at, now, payload) in [
        (3, 40, b"A".as_slice()),
        (4, 50, b"different"),
        (5, 1_021, b"A"),
    ] {
        let mut batch = v::batch(
            &scope,
            5,
            b"same",
            now,
            vec![v::create(b"birth", 20, payload)?],
            v::copy_policy(&updated),
        )?;
        batch.defaults = v::defaults(9_000);
        batch.policy_references_captured = None;
        batch.inputs[0].target = WorkflowValueTargetGuard::Present { incarnation: 999 };
        let frame = v::encode(&batch)?;
        f::append(&mut log, at, frame.as_bytes())?;
        if direct {
            check_retained_branch(v::apply(&mut state, &frame, at)?, at)?;
            if at == 5 {
                assert!(matches!(
                    v::apply(&mut state, &frame, at)?,
                    Outcome::LogReplay(_)
                ));
            }
        }
    }
    log.save_committed(f::at(5))?;
    drop((log, state, birth));
    let (log, state) = f::open_pair(path)?;
    let (log, mut state) = recover_committed(log, state, scope.group(), scope.recovery(), *limits)?;
    assert_original(&v::outcome(&mut state, b"same")?);
    assert_eq!(
        v::control(&mut state)?
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(f::at(5))
    );
    let payload = v::reference(b"birth", OriginalValueRefKind::Payload, 1)?;
    let read = v::read(&state, &[&payload])?;
    drop((state, log, payload));
    v::assert_read(&read, &[b"A"], &[Some(0)]);
    drop(f::open_pair(path)?);
    Ok(())
}

fn check_retained_branch(outcome: Outcome, at: u64) -> TestResult {
    match (at, outcome) {
        (3, Outcome::Retained { control, outcome }) => {
            assert_eq!(control.position(), f::at(3).index());
            assert_original(&outcome);
        }
        (4, Outcome::IntentConflict(control)) | (5, Outcome::Expired(control)) => {
            assert_eq!(control.position(), f::at(at).index());
        }
        _ => return Err("incorrect real retained-gate branch".into()),
    }
    Ok(())
}

fn assert_original(outcome: &wf_domain::workflow::request::RetainedOutcome) {
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.intent().version(), 8);
    assert_eq!(outcome.response_version(), 5);
    assert_eq!(outcome.original_position(), f::at(1));
    assert_eq!(outcome.original_time().get(), 20);
    assert_eq!(outcome.expires_at().get(), 1_020);
    assert_eq!(outcome.response(), [0]);
}
