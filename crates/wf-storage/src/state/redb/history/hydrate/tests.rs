//! Actual immutable corruption, snapshot interleaving and delegated read failure.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use redb::Durability;
use wf_domain::history::values::HistoryValue;
use wf_domain::value::OriginalValueNode;
use wf_ports::blob::{OriginalValuePartition, OriginalValueRefKind, render_original_value_ref};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryHydrationError as Error, WorkflowHistoryValuePage,
};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{HlcTimestamp, TimestampMillis};

use crate::state::{
    key,
    redb::{ROWS, RedbStateStore, StateStoreLimits},
};

mod backend;
mod fixtures;
use backend::fixture_data::{ReadFaultClock, database};
use fixtures::fixture_data::{
    clock, directory_for_fault, fixture, limits, publish, query, rewrite, rows,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct TestClock {
    hlc: HlcTimestamp,
    wall: TimestampMillis,
    calls: usize,
    failure: Option<StateTransactionError>,
}
impl WorkflowValueReadClock for TestClock {
    fn read_snapshot(&mut self) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError> {
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or(StateTransactionError::Limit)?;
        self.failure.map_or(Ok((self.hlc, self.wall)), Err)
    }
}

struct MutatingHistoryClock<'a> {
    store: &'a RedbStateStore,
    key: Vec<u8>,
    replacement: Vec<u8>,
    calls: usize,
}
impl WorkflowValueReadClock for MutatingHistoryClock<'_> {
    fn read_snapshot(&mut self) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError> {
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or(StateTransactionError::Limit)?;
        let mut write = self
            .store
            .database
            .begin_write()
            .map_err(|_| StateTransactionError::Unavailable)?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| StateTransactionError::Unavailable)?;
        let mut table = write
            .open_table(ROWS)
            .map_err(|_| StateTransactionError::Unavailable)?;
        let old = table
            .insert(self.key.as_slice(), self.replacement.as_slice())
            .map_err(|_| StateTransactionError::Unavailable)?;
        if old.is_none() {
            return Err(StateTransactionError::Corrupt);
        }
        drop(old);
        drop(table);
        write
            .commit()
            .map_err(|_| StateTransactionError::Unavailable)?;
        Ok((
            HlcTimestamp::new(101, 0).map_err(|_| StateTransactionError::Corrupt)?,
            TimestampMillis::new(101),
        ))
    }
}

fn reference(version: u64) -> TestResult<Vec<u8>> {
    render_original_value_ref(
        b"i",
        OriginalValueRefKind::Payload,
        version,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|e| format!("reference: {e:?}").into())
}

fn field<'a>(
    page: &'a WorkflowHistoryValuePage,
    entry: usize,
    name: &str,
) -> Option<&'a HistoryValue> {
    page.writes
        .iter()
        .find(|w| w.entry_index == entry && w.field.key() == name)
        .map(|w| &w.value)
}

#[test]
fn history_hydration_authenticates_live_values_and_lookahead_before_omission() -> TestResult {
    for fault in 0..4 {
        let (path, store, scope) = fixture(&format!("auth-{fault}"))?;
        let partition = PartitionId::new(b"p".to_vec())?;
        let id = RecordId::new(b"i".to_vec())?;
        let mut clock = clock()?;
        let healthy = store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            query(0),
            limits(),
        );
        assert!(
            healthy.is_ok(),
            "healthy history hydration: {:?}",
            healthy.as_ref().err()
        );
        assert!(healthy?.history.lookahead.is_some());
        let before = rows(&store, false)?;
        let selected = reference(if fault == 1 { 2 } else { 1 })?;
        let key = if fault < 2 {
            key::encode_immutable_key(&scope, &selected, 0, 65_535)?
        } else {
            key::encode_alias_key(&scope, &selected, 65_535)?
        };
        let mut changed = before.get(&key).ok_or("actual value row")?.clone();
        if fault == 3 {
            rewrite(&store, &key, None)?;
        } else {
            changed[0] ^= 1;
            rewrite(&store, &key, Some(&changed))?;
        }
        let result = store.read_workflow_history_page_with_values(
            &mut clock,
            &scope,
            &partition,
            &id,
            query(-1),
            limits(),
        );
        if fault == 3 {
            let missing = result?;
            assert!(matches!(
                field(&missing, 0, "payload"),
                Some(HistoryValue::Nil)
            ));
            assert!(matches!(
                field(&missing, 0, "payload_missing"),
                Some(HistoryValue::Boolean(true))
            ));
            assert!(matches!(
                field(&missing, 1, "payload_omitted"),
                Some(HistoryValue::Boolean(true))
            ));
        } else {
            assert!(
                matches!(result, Err(Error::State(StateTransactionError::Corrupt))),
                "{fault}: {:?}",
                result.as_ref().err()
            );
            assert!(matches!(
                store.read_workflow_history_page(
                    &scope,
                    &partition,
                    &id,
                    query(0).query,
                    limits().history
                ),
                Err(StateTransactionError::Unavailable)
            ));
        }
        drop(store);
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[test]
fn history_hydration_uses_one_snapshot_during_clock_publication() -> TestResult {
    let (path, store, scope) = fixture("one-snapshot")?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut ordinary = clock()?;
    let healthy = store.read_workflow_history_page_with_values(
        &mut ordinary,
        &scope,
        &partition,
        &id,
        query(100),
        limits(),
    );
    assert!(
        healthy.is_ok(),
        "healthy history hydration: {:?}",
        healthy.as_ref().err()
    );
    healthy?;
    let key = key::encode_immutable_key(&scope, &reference(1)?, 0, 65_535)?;
    assert_eq!(
        rows(&store, false)?
            .get(&key)
            .ok_or("published immutable")?,
        b"first"
    );
    let mut racing = MutatingHistoryClock {
        store: &store,
        key: key.clone(),
        replacement: b"Xirst".to_vec(),
        calls: 0,
    };
    let coherent = store.read_workflow_history_page_with_values(
        &mut racing,
        &scope,
        &partition,
        &id,
        query(100),
        limits(),
    )?;
    assert_eq!(racing.calls, 1);
    let Some(HistoryValue::Decoded(document)) = field(&coherent, 0, "payload") else {
        return Err("decoded old snapshot missing".into());
    };
    assert!(
        matches!(document.root(),Some(OriginalValueNode::Binary {bytes}) if document.byte_range(bytes.clone())==Some(b"first".as_slice()))
    );
    assert_eq!(
        rows(&store, false)?.get(&key).ok_or("committed mutation")?,
        b"Xirst"
    );
    assert!(matches!(
        store.read_workflow_history_page_with_values(
            &mut ordinary,
            &scope,
            &partition,
            &id,
            query(100),
            limits()
        ),
        Err(Error::State(StateTransactionError::Corrupt))
    ));
    drop(store);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn history_hydration_sanitizes_actual_backend_read_failure() -> TestResult {
    let directory = directory_for_fault("backend-read")?;
    let path = directory.join("state");
    let arm = Arc::new(AtomicBool::new(false));
    let hits = Arc::new(AtomicUsize::new(0));
    let (store, scope) = publish(database(&path, arm.clone(), hits.clone())?)?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut ordinary = clock()?;
    let healthy = store.read_workflow_history_page_with_values(
        &mut ordinary,
        &scope,
        &partition,
        &id,
        query(100),
        limits(),
    );
    assert!(
        healthy.is_ok(),
        "healthy history hydration: {:?}",
        healthy.as_ref().err()
    );
    healthy?;
    let before = rows(&store, false)?;
    let meta = rows(&store, true)?;
    let mut failing = ReadFaultClock { arm, calls: 0 };
    let failed = store.read_workflow_history_page_with_values(
        &mut failing,
        &scope,
        &partition,
        &id,
        query(100),
        limits(),
    )?;
    assert_eq!(failing.calls, 1);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(matches!(
        field(&failed, 0, "payload_error"),
        Some(HistoryValue::Error("ERR storage read failed"))
    ));
    assert!(field(&failed, 0, "payload").is_none());
    assert!(field(&failed, 0, "payload_missing").is_none());
    assert_eq!(rows(&store, false)?, before);
    assert_eq!(rows(&store, true)?, meta);
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
    let restored = reopened.read_workflow_history_page_with_values(
        &mut ordinary,
        &scope,
        &partition,
        &id,
        query(100),
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
