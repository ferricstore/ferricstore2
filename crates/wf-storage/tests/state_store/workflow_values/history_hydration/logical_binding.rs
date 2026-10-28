//! Real automatic-partition publication; placement is explicitly configured.

use super::super::logical_binding::automatic;
use super::*;

#[test]
fn logical_binding_publishes_automatic_partition_through_reopen() -> TestResult {
    let (encoded, intent) = automatic(b"abc")?;
    let scope = crate::fixtures::scope()?;
    let partition = PartitionId::new(b"__flow_auto__:194".to_vec())?;
    let placements = [(partition.clone(), scope.group().clone())];
    let directory = crate::fixtures::directory("logical-auto-binding")?;
    let path = directory.join("state");
    let mut store = RedbStateStore::create_stored_value_database(
        redb::Builder::new().create(&path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        f::defaults(),
        StateStoreLimits::default(),
    )?;
    let outcome = apply_workflow_stored_value_batch_transaction(
        &mut store,
        &scope,
        encoded.as_bytes(),
        crate::fixtures::log(0),
        f::limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
        return Err("retained binding publication".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.intent(), intent);
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let page = store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &RecordId::new(b"abc".to_vec())?,
            selection(10, HistoryValueReturn::Enabled { max_bytes: 1_024 }),
            limits(),
        )?;
        assert_eq!(page.history.entries.len(), 1);
        binary(&page, 0, "payload", b"FSV2\x02binary")?;
        assert!(matches!(
            field(&page, 0, "payload_size")?,
            HistoryValue::StoredSize(16)
        ));
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
