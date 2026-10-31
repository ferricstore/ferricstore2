//! Real GC2 state, committed log and whole-table observations.
use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::{active::LifecycleDefaults, control::PreparedDigest};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{RaftLogLimits, encode_catalog_progress_request},
};
use wf_storage::{
    state::{StateStoreGenesis, StateStoreLimits},
    transaction::apply_catalog_progress_transaction,
};

pub(super) type ProgressFixture = BTreeMap<String, f::Rows>;
#[derive(Clone, Copy)]
pub(super) enum ProgressFault {
    Healthy,
    TrailingHistoricalBody,
}
pub(super) fn selected() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?.with_governance_catalog_progress())
}
pub(super) fn log(dir: &Path) -> TestResult<RedbLogStore> {
    let scope = f::scope()?;
    Ok(RedbLogStore::create(
        &dir.join("log.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}
pub(super) fn pair(dir: &Path, progress: bool) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let genesis = StateStoreGenesis {
        initial_time: f::clock(0, 0, 0)?,
        placements: &[],
        policies: &[],
    };
    let database = redb::Builder::new().create(dir.join("state.redb"))?;
    let state = if progress {
        RedbStateStore::create_governance_progress_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            None,
            StateStoreLimits::default(),
        )?
    } else {
        RedbStateStore::create_governance_catalog_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            StateStoreLimits::default(),
        )?
    };
    Ok((log(dir)?, state))
}
fn defaults() -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms: 7000,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1000,
    };
    let mut hash = Sha256::new();
    hash.update(b"wf-workflow-value-defaults-v1");
    hash.update(lifecycle.ttl_ms.to_be_bytes());
    hash.update(lifecycle.history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.history_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_max_events.to_be_bytes());
    WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    }
}
fn frame(id: &[u8], time: u64, cursor: Option<&[u8]>) -> TestResult<PreparedBytes> {
    Ok(encode_catalog_progress_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        f::clock(time, 0, time)?,
        wf_domain::governance::catalog::CatalogProgressChange {
            catalog: b"limit",
            cursor,
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
pub(super) fn frames() -> TestResult<Vec<PreparedBytes>> {
    Ok(vec![
        frame(b"old", 1000, Some(b"a"))?,
        frame(b"new", 1001, Some(b"b"))?,
        frame(b"clear", 1002, None)?,
        frame(b"old", 1000, Some(b"a"))?,
        frame(b"new", 1004, Some(b"c"))?,
        frame(b"final", 1005, Some(b"d"))?,
    ])
}
pub(super) fn append(
    log: &mut RedbLogStore,
    frames: &[PreparedBytes],
    fault: ProgressFault,
) -> TestResult {
    for (index, frame) in frames.iter().enumerate() {
        let mut body = frame.as_bytes().to_vec();
        if matches!(fault, ProgressFault::TrailingHistoricalBody) && index == 2 {
            body.push(0);
        }
        f::append(log, u64::try_from(index)?, &body)?;
    }
    f::append(log, 6, b"opaque uncommitted progress suffix")?;
    log.save_committed(f::at(5))?;
    Ok(())
}
pub(super) fn apply(
    state: &mut RedbStateStore,
    frames: &[PreparedBytes],
    end: usize,
) -> TestResult {
    for (index, frame) in frames.iter().take(end).enumerate() {
        apply_catalog_progress_transaction(
            state,
            &f::scope()?,
            frame.as_bytes(),
            f::at(u64::try_from(index)?),
            f::driver_limits(),
        )?;
    }
    Ok(())
}
pub(super) fn check(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    assert_eq!(
        state.governance_catalog_progress(
            &scope,
            b"limit",
            GovernanceCatalogReadLimits::default()
        )?,
        GovernanceCatalogProgress::Cursor(b"d".to_vec())
    );
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let receipt = tx
        .outcome(scope.tenant(), &RequestId::new(b"old".to_vec())?)?
        .ok_or("missing original progress receipt")?;
    assert_eq!(
        (
            receipt.intent().version(),
            receipt.response_version(),
            receipt.original_position(),
            receipt.original_time().get()
        ),
        (15, 13, f::at(0), 1000)
    );
    assert_eq!(receipt.response(), b"FWPR\0\x01");
    assert_eq!(tx.control()?.position(), f::at(5).index());
    Ok(())
}
pub(super) fn cohort(dir: &Path) -> TestResult<ProgressFixture> {
    let db = redb::Builder::new().open(dir.join("state.redb"))?;
    let tx = db.begin_read()?;
    assert_eq!(tx.list_multimap_tables()?.count(), 0);
    let mut result = BTreeMap::new();
    for handle in tx.list_tables()? {
        let name = handle.name();
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (k, v) = row?;
            assert!(
                rows.insert(k.value().to_vec(), v.value().to_vec())
                    .is_none()
            );
        }
        assert!(result.insert(name.to_owned(), rows).is_none());
    }
    assert_eq!(result.len(), 6);
    Ok(result)
}

pub(super) fn damage_hint(dir: &Path) -> TestResult {
    let db = redb::Builder::new().open(dir.join("state.redb"))?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        let mut selected = None;
        for row in table.iter()? {
            let (key, _) = row?;
            if key.value().get(6) == Some(&19) {
                selected = Some(key.value().to_vec());
                break;
            }
        }
        let key = selected.ok_or("missing progress hint")?;
        assert!(
            table
                .insert(key.as_slice(), b"invalid-hint".as_slice())?
                .is_some()
        );
    }
    tx.commit()?;
    Ok(())
}
