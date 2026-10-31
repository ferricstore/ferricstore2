//! Real GC2 state, committed log and whole-table observations.
use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::{active::LifecycleDefaults, control::PreparedDigest};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{RaftLogLimits, encode_catalog_prune_request},
};
use wf_storage::{
    state::{StateStoreGenesis, StateStoreLimits},
    transaction::apply_catalog_prune_transaction,
};

pub(super) type PruneHistoryFixture = BTreeMap<String, f::Rows>;
#[derive(Clone, Copy)]
pub(super) enum PruneHistoryFault {
    Healthy,
    TrailingHistoricalBody,
}
pub(super) fn selected() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?.with_governance_catalog_pruning())
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
        RedbStateStore::create_governance_prune_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            None,
            StateStoreLimits::default(),
        )?
    } else {
        RedbStateStore::create_governance_progress_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            None,
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
fn registration(id: &[u8], time: u64, member: &[u8]) -> TestResult<PreparedBytes> {
    Ok(wf_ports::log::encode_catalog_membership_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        f::clock(time, 0, time)?,
        wf_domain::governance::catalog::CatalogMembershipChange {
            catalog: b"limit",
            action: wf_domain::governance::catalog::CatalogMembershipAction::Register,
            members: &[member],
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
fn prune(
    id: &[u8],
    time: u64,
    expected: wf_domain::workflow::control::EntryIdentity,
) -> TestResult<PreparedBytes> {
    Ok(encode_catalog_prune_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        f::clock(time, 0, time)?,
        wf_domain::governance::catalog::CatalogPruneRequest {
            catalog: b"limit",
            member: b"a",
            expected,
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
pub(super) fn frames() -> TestResult<Vec<PreparedBytes>> {
    use wf_domain::workflow::control::EntryIdentity;
    let first = registration(b"add-a", 1000, b"a")?;
    let original = EntryIdentity::new(f::at(0), first.digest());
    let new = registration(b"new-a", 1002, b"a")?;
    let later = EntryIdentity::new(f::at(2), new.digest());
    Ok(vec![
        first,
        prune(b"old", 1001, original)?,
        new,
        prune(b"old", 1001, original)?,
        prune(b"stale", 1004, original)?,
        registration(b"add-b", 1005, b"b")?,
        prune(b"exact", 1006, later)?,
        prune(b"missing", 1007, later)?,
        registration(b"final", 1008, b"a")?,
    ])
}
pub(super) fn append(
    log: &mut RedbLogStore,
    frames: &[PreparedBytes],
    fault: PruneHistoryFault,
) -> TestResult {
    for (index, frame) in frames.iter().enumerate() {
        let mut body = frame.as_bytes().to_vec();
        if matches!(fault, PruneHistoryFault::TrailingHistoricalBody) && index == 1 {
            body.push(0);
        }
        f::append(log, u64::try_from(index)?, &body)?;
    }
    f::append(log, 9, b"opaque uncommitted progress suffix")?;
    log.save_committed(f::at(8))?;
    Ok(())
}
pub(super) fn apply(
    state: &mut RedbStateStore,
    frames: &[PreparedBytes],
    end: usize,
) -> TestResult {
    for (index, frame) in frames.iter().take(end).enumerate() {
        if frame.as_bytes().get(6) == Some(&12) {
            wf_storage::transaction::apply_catalog_membership_transaction(
                state,
                &f::scope()?,
                frame.as_bytes(),
                f::at(u64::try_from(index)?),
                f::driver_limits(),
            )?;
        } else {
            apply_catalog_prune_transaction(
                state,
                &f::scope()?,
                frame.as_bytes(),
                f::at(u64::try_from(index)?),
                f::driver_limits(),
            )?;
        }
    }
    Ok(())
}
pub(super) fn check(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let page = state.governance_catalog_page(
        wf_ports::state::GovernanceCatalogRead {
            scope: &scope,
            catalog: b"limit",
            cursor: None,
            limit: 256,
        },
        GovernanceCatalogReadLimits::default(),
    )?;
    assert_eq!(page.members, vec![b"a".to_vec(), b"b".to_vec()]);
    let observed = state.governance_catalog_prune_page(
        wf_ports::state::GovernanceCatalogRead {
            scope: &scope,
            catalog: b"limit",
            cursor: None,
            limit: 1,
        },
        GovernanceCatalogReadLimits::default(),
    )?;
    assert_eq!(
        observed.revision,
        Some(wf_domain::workflow::control::EntryIdentity::new(
            f::at(8),
            frames()?[8].digest()
        ))
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
        (16, 14, f::at(1), 1001)
    );
    assert_eq!(receipt.response(), b"FWXR\0\x01\0");
    assert_eq!(tx.control()?.position(), f::at(8).index());
    Ok(())
}
pub(super) fn cohort(dir: &Path) -> TestResult<PruneHistoryFixture> {
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
