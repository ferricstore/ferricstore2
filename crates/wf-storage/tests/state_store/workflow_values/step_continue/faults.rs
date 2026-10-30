use super::{fixtures as f, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_storage::transaction::RecordBatchTransactionError;

#[test]
fn uncertain_history_upgrade_reopens_one_complete_format_and_preserves_rows() -> TestResult {
    let directory = common::directory("step-upgrade-uncertain")?;
    drop(f::fresh(&directory.join("healthy"))?);
    let path = directory.join("state");
    let mut store = values::fresh(&path)??;
    f::seed(&mut store)?;
    drop(store);
    let original = common::raw_rows(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let wrapped = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert!(matches!(
        wrapped.upgrade_history_snapshot_layout(),
        Err(StateTransactionError::Uncertain)
    ));
    assert_eq!(common::raw_rows(&path)?, original);
    let manifest = values::raw_manifest(&path)?;
    assert!(manifest.starts_with(b"FWSS\0\x07") || manifest.starts_with(b"FWSS\0\x08"));
    let mut recovered = f::upgrade(common::open(&path)?)?;
    assert_eq!(f::record(&mut recovered)?.record.version(), 1);
    drop(recovered);
    assert_eq!(common::raw_rows(&path)?, original);
    assert!(values::raw_manifest(&path)?.starts_with(b"FWSS\0\x08"));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn durable_step_uncertain_sync_fences_and_reopens_one_complete_publication() -> TestResult {
    let directory = common::directory("step-uncertain")?;
    let path = directory.join("state");
    let mut store = f::fresh(&path)?;
    f::seed(&mut store)?;
    drop(store);
    let fault = Arc::new(AtomicU8::new(0));
    let mut wrapped = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    let frame = f::frame(2, Some(b"new"), b"w:0:1")?;
    fault.store(1, Ordering::SeqCst);
    let result = apply_step_continue_transaction(
        &mut wrapped,
        &common::scope()?,
        frame.as_bytes(),
        common::log(1),
        f::limits(),
    );
    assert!(matches!(
        result,
        Err(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain)
        ))
    ));
    let scope = common::scope()?;
    assert!(matches!(
        wrapped.begin_apply(
            scope.group(),
            scope.recovery(),
            f::limits().record_batch.codec.admission(),
            1000,
            common::MIB,
            common::MIB
        ),
        Err(StateTransactionError::Unavailable)
    ));
    drop(wrapped);
    let mut reopened = common::open(&path)?;
    let version = f::record(&mut reopened)?.record.version();
    assert!([1, 2].contains(&version));
    assert_eq!(
        f::history(&reopened)?.entries.len(),
        usize::try_from(version)?
    );
    let observed = reopened
        .begin_apply(
            scope.group(),
            scope.recovery(),
            f::limits().record_batch.codec.admission(),
            1000,
            common::MIB,
            common::MIB,
        )?
        .control()?;
    assert_eq!(observed.position().get(), version - 1);
    assert_eq!(
        observed.last_entry().ok_or("reopened entry")?.log(),
        common::log(version - 1)
    );
    if version == 2 {
        assert!(matches!(
            f::apply(&mut reopened, &frame, 1)?,
            RecordBatchTransactionOutcome::LogReplay(_)
        ));
    }
    let acknowledged = f::retained(f::apply(&mut reopened, &frame, version)?)?;
    assert_eq!(acknowledged.status(), RetainedStatus::Accepted);
    assert_eq!(f::record(&mut reopened)?.record.version(), 2);
    assert_eq!(f::history(&reopened)?.entries.len(), 2);
    drop(reopened);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
