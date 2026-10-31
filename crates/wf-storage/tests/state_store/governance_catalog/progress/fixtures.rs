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
