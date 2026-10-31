use super::super::fixtures as leaf;
use super::{fixtures as a, *};
use wf_ports::log::inspect_atomic_claim_reply;
use wf_storage::transaction::apply_atomic_claim_transaction;

#[test]
fn operational_read_refusal_keeps_the_entire_original_cohort_and_no_receipt() -> TestResult {
    let dir = f::directory("atomic-claim-read-refusal")?;
    let path = dir.join("state");
    drop(a::filled(&path)?);
    let before = leaf::cohort(&path)?;
    let mut store = f::open(&path)?;
    let frame = a::frame(100)?;
    let mut limits = a::limits();
    limits.values.record_batch.maximum_read_steps = 1;
    assert_eq!(
        apply_atomic_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(5),
            limits,
        )
        .err(),
        Some(wf_storage::transaction::ClaimTransactionError::Transaction(
            wf_ports::state::StateTransactionError::Limit,
        ))
    );
    drop(store);
    assert_eq!(leaf::cohort(&path)?, before);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn later_phase_budget_failure_retains_one_error_and_no_record_value_or_history_prefix() -> TestResult
{
    let dir = f::directory("atomic-claim-late-budget")?;
    let path = dir.join("state");
    let store = a::filled(&path)?;
    drop(store);
    let before = leaf::cohort(&path)?;
    let mut store = f::open(&path)?;
    let frame = a::frame(1)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        a::limits(),
    )?)?;
    assert!(matches!(outcome.status(), RetainedStatus::ClaimRejected(_)));
    let reply = inspect_atomic_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!((reply.count(), reply.phase_count()), (0, 0));
    assert!(reply.used_indices().next().is_none());
    drop(store);
    let after = leaf::cohort(&path)?;
    let controls: Vec<_> = before[1]
        .iter()
        .filter(|(_, value)| value.starts_with(b"FWSC\0\x01"))
        .collect();
    assert_eq!(controls.len(), 1);
    let (control_key, _) = controls[0];
    for (key, value) in &before[1] {
        if key != control_key {
            assert_eq!(after[1].get(key), Some(value), "preexisting row {key:?}");
        }
    }
    let scope = f::scope()?;
    let control = wf_storage::state::value::decode_control(
        after[1].get(control_key).ok_or("control row")?,
        scope.group(),
        scope.recovery(),
        1024,
    )?;
    assert_eq!(control.position().get(), 5);
    assert_eq!(control.time().wall().get(), 3000);
    let prepared = wf_ports::log::prepare_atomic_claim_request(
        frame.as_bytes(),
        v::limits().record_batch.codec,
        wf_domain::claim::caller::ClaimCallerLimits::default(),
        v::limits().record_batch.workspace_bytes,
    )?;
    assert_eq!(
        control.last_entry().ok_or("identity")?.prepared(),
        prepared.digest()
    );
    assert_eq!(after[1].len(), before[1].len() + 1);
    let extra = after[1]
        .iter()
        .find(|(key, _)| !before[1].contains_key(*key))
        .ok_or("one parent outcome")?;
    assert!(extra.1.starts_with(b"FWSO\0\x01"));
    assert_eq!(after[0], before[0]);
    assert_eq!(after[2..], before[2..]);
    let mut reopened = f::open(&path)?;
    assert_eq!(leaf::record(&mut reopened, b"r1")?.record.version(), 2);
    assert_eq!(leaf::record(&mut reopened, b"q1")?.record.version(), 1);
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn uncertain_parent_commit_fences_and_recovers_only_the_complete_old_or_new_cohort() -> TestResult {
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };
    use wf_ports::state::{GroupApplyStore, StateTransactionError as S};
    let dir = f::directory("atomic-claim-uncertain-parent")?;
    let path = dir.join("state");
    let reference = dir.join("reference");
    drop(a::filled(&path)?);
    let before = leaf::cohort(&path)?;
    let frame = a::frame(100)?;
    let mut expected = a::filled(&reference)?;
    apply_atomic_claim_transaction(
        &mut expected,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        a::limits(),
    )?;
    drop(expected);
    let complete = leaf::cohort(&reference)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_atomic_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(5),
            a::limits()
        )
        .err(),
        Some(wf_storage::transaction::ClaimTransactionError::Transaction(
            S::Uncertain
        ))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    let scope = f::scope()?;
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits().record_batch.codec.admission(),
            1000,
            1024 * 1024,
            1024 * 1024
        ),
        Err(S::Unavailable)
    ));
    drop(store);
    let actual = leaf::cohort(&path)?;
    assert!(
        actual == before || actual == complete,
        "no partly committed phase/receipt cohort"
    );
    let mut restored = f::open(&path)?;
    let running = leaf::record(&mut restored, b"r1")?.record.version();
    let queued = leaf::record(&mut restored, b"q1")?.record.version();
    assert!(matches!((running, queued), (2, 1) | (3, 2)));
    drop(restored);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
