//! A later projection refusal cannot undo an observed fatal read failure.

use wf_domain::history::values::HistoryValueError;

use super::*;

#[test]
fn history_hydration_fences_read_failure_after_projection_limit() -> TestResult {
    let directory = directory_for_fault("read-then-projection-limit")?;
    let path = directory.join("state");
    let arm = Arc::new(AtomicBool::new(false));
    let hits = Arc::new(AtomicUsize::new(0));
    let (store, scope) = publish_for_read_fault(database(&path, arm.clone(), hits.clone())?)?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut ordinary = clock()?;
    let healthy = store.read_workflow_history_page_with_values(
        &mut ordinary,
        &scope,
        &partition,
        &id,
        query(262_144),
        limits(),
    );
    assert!(
        healthy.is_ok(),
        "healthy history hydration: {:?}",
        healthy.as_ref().err()
    );
    healthy?;
    let before = rows(&store, false)?;
    let mut failing = ReadFaultClock { arm, calls: 0 };
    let mut short = limits();
    short.projection.maximum_writes = 1;
    let result = store.read_workflow_history_page_with_values(
        &mut failing,
        &scope,
        &partition,
        &id,
        query(262_144),
        short,
    );
    assert_eq!(failing.calls, 1);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(
        matches!(result, Err(Error::Projection(HistoryValueError::Limit))),
        "{:?}",
        result.as_ref().err()
    );
    assert!(matches!(
        store.read_workflow_history_page(&scope, &partition, &id, query(0).query, limits().history),
        Err(StateTransactionError::Unavailable)
    ));
    drop(store);
    let reopened = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    assert_eq!(rows(&reopened, false)?, before);
    let restored = reopened.read_workflow_history_page_with_values(
        &mut ordinary,
        &scope,
        &partition,
        &id,
        query(262_144),
        limits(),
    )?;
    assert!(matches!(
        field(&restored, 0, "payload"),
        Some(HistoryValue::Decoded(_))
    ));
    drop(reopened);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
