use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::{
    active::LifecycleDefaults,
    control::PreparedDigest,
    request::{RequestScope, RetainedOutcome},
};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{PreparedBytes, encode_catalog_membership_request},
    state::{GovernanceCatalogPage, GovernanceCatalogRead},
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_types::identity::RequestId;

type Cohort = BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>;

pub(super) fn create(path: &Path) -> TestResult<RedbStateStore> {
    let scope = common::scope()?;
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
    let defaults = WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    };
    Ok(RedbStateStore::create_governance_catalog_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &common::placements()?,
            policies: &[],
        },
        defaults,
        StateStoreLimits::default(),
    )?)
}
pub(super) fn frame(id: &[u8], action: A, members: &[&[u8]]) -> TestResult<PreparedBytes> {
    Ok(encode_catalog_membership_request(
        &common::scope()?,
        &RequestId::new(id.to_vec())?,
        common::clock(1000)?,
        C {
            catalog: b"limit",
            action,
            members,
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?)
}
pub(super) fn retained(
    path: &Path,
    frame: &PreparedBytes,
    index: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let mut store = common::open(path)?;
    let result = apply_catalog_membership_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        common::driver_limits(),
    )?;
    let Outcome::Retained { outcome, control } = result else {
        return Err("missing retained membership response".into());
    };
    assert_eq!(control.position(), common::log(index).index());
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (14, 12)
    );
    assert_eq!(outcome.response(), b"FWGR\0\x01");
    Ok(outcome)
}
pub(super) fn read<'a>(
    scope: &'a RequestScope,
    cursor: Option<&'a [u8]>,
    limit: u16,
) -> GovernanceCatalogRead<'a> {
    GovernanceCatalogRead {
        scope,
        catalog: b"limit",
        cursor,
        limit,
    }
}
pub(super) fn page(
    path: &Path,
    cursor: Option<&[u8]>,
    limit: u16,
) -> TestResult<GovernanceCatalogPage> {
    Ok(common::open(path)?.read_governance_catalog(
        read(&common::scope()?, cursor, limit),
        10000,
        64 * common::MIB,
    )?)
}
pub(super) fn cohort(path: &Path) -> TestResult<Cohort> {
    let db = redb::Builder::new().open(path)?;
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
    assert!(result.contains_key("wf_state_rows_v1"));
    assert!(result.contains_key("wf_state_meta_v1"));
    Ok(result)
}
pub(super) fn downgrade(path: &Path) -> TestResult {
    let db = redb::Builder::new().open(path)?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(common::META)?;
        let mut bytes = table
            .get(b"manifest".as_slice())?
            .ok_or("missing manifest")?
            .value()
            .to_vec();
        let end = bytes.len() - 36;
        assert_eq!(&bytes[end..end + 4], b"GC\0\x01");
        bytes.drain(end..end + 4);
        let checksum = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&checksum);
        table.insert(b"manifest".as_slice(), bytes.as_slice())?;
    }
    tx.commit()?;
    Ok(())
}
