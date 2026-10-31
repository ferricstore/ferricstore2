use super::*;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use wf_domain::workflow::{active::LifecycleDefaults, control::PreparedDigest};
use wf_ports::consensus::WorkflowValueDefaults;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
pub(super) struct PruneFixture {
    pub(super) dir: PathBuf,
    pub(super) path: PathBuf,
    pub(super) store: RedbStateStore,
}
impl PruneFixture {
    pub(super) fn new(count: Option<u64>, gc3: bool) -> TestResult<Self> {
        let dir = common::directory("gc3-prune")?;
        let path = dir.join("state");
        let scope = common::scope()?;
        let genesis = StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &[],
            policies: &[],
        };
        let database = redb::Builder::new().create(&path)?;
        let store = if gc3 {
            RedbStateStore::create_governance_prune_database(
                database,
                scope.group().clone(),
                scope.recovery().clone(),
                genesis,
                defaults(),
                count,
                StateStoreLimits::default(),
            )?
        } else {
            RedbStateStore::create_governance_progress_database(
                database,
                scope.group().clone(),
                scope.recovery().clone(),
                genesis,
                defaults(),
                count,
                StateStoreLimits::default(),
            )?
        };
        Ok(Self { dir, path, store })
    }
    pub(super) fn cleanup(self) -> TestResult {
        let Self { dir, store, .. } = self;
        drop(store);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    pub(super) fn page(&self) -> TestResult<wf_ports::state::GovernanceCatalogPrunePage> {
        Ok(self.store.governance_catalog_prune_page(
            GovernanceCatalogRead {
                scope: &common::scope()?,
                catalog: b"limit",
                cursor: None,
                limit: 1,
            },
            L::default(),
        )?)
    }
    pub(super) fn register(
        &mut self,
        id: &[u8],
        name: &[u8],
        index: u64,
    ) -> TestResult<EntryIdentity> {
        let frame = f::frame(id, A::Register, &[name])?;
        let identity = EntryIdentity::new(common::log(index), frame.digest());
        super::super::apply_catalog_membership_transaction(
            &mut self.store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(index),
            common::driver_limits(),
        )?;
        Ok(identity)
    }
    pub(super) fn frame(
        &self,
        id: &[u8],
        member: &[u8],
        expected: EntryIdentity,
    ) -> TestResult<wf_ports::log::PreparedBytes> {
        Ok(wf_ports::log::encode_catalog_prune_request(
            &common::scope()?,
            &wf_types::identity::RequestId::new(id.to_vec())?,
            common::clock(1000)?,
            C {
                catalog: b"limit",
                member,
                expected,
            },
            common::driver_limits().codec,
            16 * common::MIB,
        )?)
    }
    pub(super) fn apply(
        &mut self,
        frame: &wf_ports::log::PreparedBytes,
        index: u64,
    ) -> TestResult<Box<wf_domain::workflow::request::RetainedOutcome>> {
        let Outcome::Retained { outcome, .. } =
            wf_storage::transaction::apply_catalog_prune_transaction(
                &mut self.store,
                &common::scope()?,
                frame.as_bytes(),
                common::log(index),
                common::driver_limits(),
            )?
        else {
            return Err("missing guarded-prune retained reply".into());
        };
        Ok(outcome)
    }
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
