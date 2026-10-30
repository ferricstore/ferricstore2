use super::{fixtures as s, *};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::StateStoreLimits;
use wf_storage::transaction::RecordBatchTransactionError;

#[test]
fn stored_step_complete_snapshot_restores_original_receipt_and_typed_history() -> TestResult {
    let dir = f::directory("stored-step-snapshot")?;
    let path = dir.join("source");
    let mut store = c::fresh(&path, true)?;
    s::seed(&mut store)?;
    let frame = s::frame(Some(&s::map(b"charge_card")?))?;
    let original = s::apply(&mut store, &frame, 2)?;
    let mut wire = Vec::new();
    store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let stage = RedbSnapshotStage::receive(
        &dir.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target = dir.join("target");
    let mut restored = c::fresh(&target, true)?;
    restored.install_snapshot(stage, SnapshotLimits::default())?;
    s::assert_history(&c::history(&restored, b"i")?, &c::history(&store, b"i")?)?;
    s::assert_map(&restored, 3, b"charge_card")?;
    assert_eq!(s::apply(&mut restored, &frame, 3)?, original);
    drop((store, restored));
    drop(f::open(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn stored_step_uncertain_sync_fences_and_reopens_complete_old_or_new_cohort() -> TestResult {
    let dir = f::directory("stored-step-uncertain")?;
    let path = dir.join("state");
    let mut store = c::fresh(&path, true)?;
    s::seed(&mut store)?;
    let frame = s::frame(Some(&s::map(b"charge_card")?))?;
    // Positive control precedes fault setup and is an independent real database.
    let mut healthy = c::fresh(&dir.join("healthy"), true)?;
    s::seed(&mut healthy)?;
    s::apply(&mut healthy, &frame, 2)?;
    drop((store, healthy));
    let fault = Arc::new(AtomicU8::new(0));
    let mut wrapped = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(1, Ordering::SeqCst);
    assert_eq!(
        apply_stored_step_continue_transaction(
            &mut wrapped,
            &f::scope()?,
            frame.as_bytes(),
            f::log(2),
            v::limits()
        )
        .err(),
        Some(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain)
        ))
    );
    let scope = f::scope()?;
    assert!(matches!(
        wrapped.begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits().record_batch.codec.admission(),
            1000,
            f::MIB,
            f::MIB
        ),
        Err(StateTransactionError::Unavailable)
    ));
    drop(wrapped);
    let mut reopened = f::open(&path)?;
    let version = c::record(&mut reopened, b"i")?.record.version();
    assert!([2, 3].contains(&version));
    assert_eq!(
        c::history(&reopened, b"i")?.entries.len(),
        usize::try_from(version)?
    );
    let control = reopened
        .begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits().record_batch.codec.admission(),
            1000,
            f::MIB,
            f::MIB,
        )?
        .control()?;
    assert_eq!(control.position(), f::log(version - 1).index());
    s::apply(&mut reopened, &frame, version)?;
    assert_eq!(c::record(&mut reopened, b"i")?.record.version(), 3);
    s::assert_map(&reopened, 3, b"charge_card")?;
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
