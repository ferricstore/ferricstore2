use super::*;
use std::path::Path;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_types::identity::RequestId;

pub(super) fn create(path: &Path, count: Option<u64>) -> TestResult<RedbStateStore> {
    let scope = common::scope()?;
    Ok(RedbStateStore::create_governance_progress_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &common::placements()?,
            policies: &[],
        },
        defaults(),
        count,
        StateStoreLimits::default(),
    )?)
}
pub(super) fn frame(id: &[u8], cursor: Option<&[u8]>) -> TestResult<wf_ports::log::PreparedBytes> {
    Ok(wf_ports::log::encode_catalog_progress_request(
        &common::scope()?,
        &RequestId::new(id.to_vec())?,
        common::clock(1000)?,
        C {
            catalog: b"limit",
            cursor,
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?)
}
pub(super) fn read(store: &RedbStateStore) -> TestResult<P> {
    Ok(store.governance_catalog_progress(&common::scope()?, b"limit", L::default())?)
}
pub(super) fn apply(
    store: &mut RedbStateStore,
    frame: &wf_ports::log::PreparedBytes,
    index: u64,
) -> TestResult<Outcome> {
    Ok(apply_catalog_progress_transaction(
        store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        common::driver_limits(),
    )?)
}

fn defaults() -> wf_ports::consensus::WorkflowValueDefaults {
    use sha2::{Digest, Sha256};
    use wf_domain::workflow::{active::LifecycleDefaults, control::PreparedDigest};
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
    wf_ports::consensus::WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    }
}

pub(super) fn damage_hint(path: &Path) -> TestResult {
    use redb::{ReadableTable, TableDefinition};
    let db = redb::Builder::new().open(path)?;
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
        table.insert(key.as_slice(), b"invalid-hint".as_slice())?;
    }
    tx.commit()?;
    Ok(())
}
pub(super) fn downgrade(path: &Path) -> TestResult {
    use redb::ReadableTable;
    use sha2::{Digest, Sha256};
    let db = redb::Builder::new().open(path)?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(common::META)?;
        let mut bytes = table
            .get(b"manifest".as_slice())?
            .ok_or("missing manifest")?
            .value()
            .to_vec();
        let at = bytes.len() - 36;
        assert_eq!(&bytes[at..at + 4], b"GC\0\x02");
        bytes[at + 3] = 1;
        let end = bytes.len() - 32;
        let checksum = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&checksum);
        table.insert(b"manifest".as_slice(), bytes.as_slice())?;
    }
    tx.commit()?;
    Ok(())
}
