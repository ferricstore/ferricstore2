//! Real combined GC2/credit2 state and committed private commands.
use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use wf_domain::{
    governance::{
        credit::CreditLimits,
        identified::{
            CreditConfiguration, IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
        },
    },
    workflow::{active::LifecycleDefaults, control::PreparedDigest, request::RetainedOutcome},
};
use wf_node::recovery::{
    CommittedRecoveryLimits, purge_snapshot, recover_snapshotted, restore_received_snapshot,
};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{
        PreparedBytes, RaftLogLimits, RaftLogStore, encode_catalog_membership_request,
        encode_catalog_progress_request, encode_credit_request,
    },
    snapshot::{SnapshotLimits, StateSnapshotStore},
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{
    log::RedbLogStore,
    snapshot::RedbSnapshotStage,
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
};
use wf_types::identity::RequestId;

pub(super) type CleanupStore = BTreeMap<String, f::Rows>;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CleanupCall {
    Cleanup,
    Prune,
    Progress,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CleanupFault {
    None,
    ReadProgress,
    RefuseCleanup,
    LoseProgress,
    RefuseProgress,
    StaleTime,
}
pub(super) struct CleanupFixture {
    pub(super) root: PathBuf,
    pub(super) scope: RequestScope,
    pub(super) calls: RefCell<Vec<CleanupCall>>,
    pub(super) fault: Cell<CleanupFault>,
    pub(super) next_request: u64,
    pub(super) last_error: Option<String>,
}
pub(super) fn caps() -> TestResult<CommittedRecoveryLimits> {
    Ok(super::super::caps()?
        .with_identified_credit_catalog_limits(4, CreditLimits::default())?
        .with_governance_catalog_progress())
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
impl CleanupFixture {
    pub(super) fn new() -> TestResult<Self> {
        let root = f::directory("credit-cleaner")?;
        let scope = f::scope()?;
        drop(RedbLogStore::create(
            &root.join("log.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            RaftLogLimits::default(),
        )?);
        drop(RedbStateStore::create_governance_progress_database(
            redb::Builder::new().create(root.join("state.redb"))?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0, 0, 0)?,
                placements: &[],
                policies: &[],
            },
            defaults(),
            Some(4),
            StateStoreLimits::default(),
        )?);
        Ok(Self {
            root,
            scope,
            calls: RefCell::new(Vec::new()),
            fault: Cell::new(CleanupFault::None),
            next_request: 0,
            last_error: None,
        })
    }
    pub(super) fn open(&self) -> TestResult<(RedbLogStore, RedbStateStore)> {
        f::open_pair(&self.root)
    }
    pub(super) fn commit(
        &self,
        request: &RequestId,
        frame: PreparedBytes,
    ) -> TestResult<Box<RetainedOutcome>> {
        let (mut log, state) = self.open()?;
        let at = log.state()?.last_log().map_or(0, |id| id.index().get() + 1);
        f::append(&mut log, at, frame.as_bytes())?;
        log.save_committed(f::at(at))?;
        drop(frame);
        let (log, mut state) = recover_snapshotted(
            log,
            state,
            self.scope.group(),
            self.scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        let tx = state.begin_apply(
            self.scope.group(),
            self.scope.recovery(),
            f::driver_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?;
        let outcome = tx
            .outcome(self.scope.tenant(), request)?
            .ok_or("missing actual cleaner receipt")?;
        drop(tx);
        drop((log, state));
        Ok(Box::new(outcome))
    }
    fn identity(&mut self) -> TestResult<RequestId> {
        self.next_request += 1;
        Ok(RequestId::new(
            format!("cleaner-{}", self.next_request).into_bytes(),
        )?)
    }
    pub(super) fn seed(&mut self, name: &[u8], amount: usize) -> TestResult<Vec<Vec<u8>>> {
        let request = self.identity()?;
        let amount64 = u64::try_from(amount)?;
        self.commit(
            &request,
            encode_credit_request(
                &self.scope,
                &request,
                f::clock(1000, 0, 1000)?,
                IdentifiedCreditRequest {
                    scope: name,
                    shard_count: 4,
                    configuration: Some(CreditConfiguration {
                        limit: amount64,
                        config_version: Some(1),
                        policy_fingerprint: None,
                    }),
                    mutation: Op::Lease {
                        shard_id: 0,
                        amount: amount64,
                        ttl_ms: 5,
                    },
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?,
        )?;
        let ids = (0..amount)
            .map(|n| [name, format!("-{n:04}").as_bytes()].concat())
            .collect::<Vec<_>>();
        let request = self.identity()?;
        self.commit(
            &request,
            encode_credit_request(
                &self.scope,
                &request,
                f::clock(1001, 0, 1001)?,
                IdentifiedCreditRequest {
                    scope: name,
                    shard_count: 4,
                    configuration: None,
                    mutation: Op::Spend {
                        shard_id: 0,
                        lease_epoch: 1,
                        amount: amount64,
                        ttl_ms: None,
                        reservation_ids: &ids,
                    },
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?,
        )?;
        self.register(name)?;
        Ok(ids)
    }
    pub(super) fn member(&self, name: &[u8]) -> TestResult<Vec<u8>> {
        let encoded =
            wf_ports::state::encode_credit_catalog_members(&self.scope, &[name], 65535, 131072)?;
        Ok(encoded
            .members()
            .next()
            .ok_or("missing encoded member")?
            .to_vec())
    }
    pub(super) fn register(&mut self, name: &[u8]) -> TestResult {
        use wf_domain::governance::catalog::{CatalogMembershipAction, CatalogMembershipChange};
        let member = self.member(name)?;
        let request = self.identity()?;
        self.commit(
            &request,
            encode_catalog_membership_request(
                &self.scope,
                &request,
                f::clock(1002, 0, 1002)?,
                CatalogMembershipChange {
                    catalog: b"limit",
                    action: CatalogMembershipAction::Register,
                    members: &[&member],
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?,
        )?;
        Ok(())
    }
    pub(super) fn set_progress(&mut self, cursor: Option<&[u8]>) -> TestResult {
        let request = self.identity()?;
        self.commit(
            &request,
            encode_catalog_progress_request(
                &self.scope,
                &request,
                f::clock(1003, 0, 1003)?,
                wf_domain::governance::catalog::CatalogProgressChange {
                    catalog: b"limit",
                    cursor,
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?,
        )?;
        Ok(())
    }
    pub(super) fn progress(&self) -> TestResult<GovernanceCatalogProgress> {
        let (_, state) = self.open()?;
        Ok(state.governance_catalog_progress(
            &self.scope,
            b"limit",
            GovernanceCatalogReadLimits::default(),
        )?)
    }
    pub(super) fn run(&mut self, budget: u16) -> TestResult<Stats> {
        let scope = self.scope.clone();
        let sources = [CreditCleanupSource {
            scope: &scope,
            shard_count: 4,
        }];
        let result = run_credit_cleanup_tick(
            CreditCleanupRequest {
                target: &scope,
                sources: &sources,
                catalog: b"limit",
                now_ms: 1006,
            },
            self,
            Some(budget),
            limits(),
        );
        if let Some(error) = self.last_error.take() {
            return Err(error.into());
        }
        Ok(result?)
    }
    pub(super) fn remaining(&self, name: &[u8]) -> TestResult<BTreeSet<Vec<u8>>> {
        let db = redb::Builder::new().open(self.root.join("state.redb"))?;
        let tx = db.begin_read()?;
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        let mut result = BTreeSet::new();
        for row in table.iter()? {
            let (key, _) = row?;
            if key.value().get(6) == Some(&13) {
                let mut parts = physical_components(key.value())?;
                assert_eq!(parts.len(), 7);
                if parts[3] == name {
                    assert!(result.insert(parts.pop().ok_or("missing reservation ID")?));
                }
            }
        }
        Ok(result)
    }
    pub(super) fn cohort(&self) -> TestResult<CleanupStore> {
        let mut result = BTreeMap::new();
        for (file, prefix) in [("state.redb", "state"), ("log.redb", "log")] {
            let db = redb::Builder::new().open(self.root.join(file))?;
            let tx = db.begin_read()?;
            for handle in tx.list_tables()? {
                let name = handle.name();
                let mut rows = f::Rows::new();
                if name == "wf_raft_log_v1" {
                    let table = tx.open_table(TableDefinition::<u64, &[u8]>::new(name))?;
                    for row in table.iter()? {
                        let (k, v) = row?;
                        rows.insert(k.value().to_be_bytes().to_vec(), v.value().to_vec());
                    }
                } else {
                    let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
                    for row in table.iter()? {
                        let (k, v) = row?;
                        rows.insert(k.value().to_vec(), v.value().to_vec());
                    }
                }
                assert!(result.insert(format!("{prefix}/{name}"), rows).is_none());
            }
        }
        assert_eq!(result.len(), 8);
        Ok(result)
    }
    pub(super) fn damage_hint(&self) -> TestResult {
        let db = redb::Builder::new().open(self.root.join("state.redb"))?;
        let tx = db.begin_write()?;
        {
            let mut table =
                tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
            let mut key = None;
            for row in table.iter()? {
                let (k, _) = row?;
                if k.value().get(6) == Some(&19) {
                    key = Some(k.value().to_vec());
                    break;
                }
            }
            let key = key.ok_or("missing hint")?;
            table.insert(key.as_slice(), b"invalid-hint".as_slice())?;
        }
        tx.commit()?;
        Ok(())
    }
    pub(super) fn snapshot(&self) -> TestResult {
        let (log, mut state) = self.open()?;
        let mut wire = Vec::new();
        state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
        let (log, state) = purge_snapshot(
            log,
            state,
            self.scope.group(),
            self.scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        drop(recover_snapshotted(
            log,
            state,
            self.scope.group(),
            self.scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?);
        let child = self.root.join("received");
        std::fs::create_dir(&child)?;
        let log = RedbLogStore::create(
            &child.join("log.redb"),
            self.scope.group().clone(),
            self.scope.recovery().clone(),
            RaftLogLimits::default(),
        )?;
        let candidate = RedbSnapshotStage::receive(
            &child.join("state.redb"),
            self.scope.group().clone(),
            self.scope.recovery().clone(),
            &mut std::io::Cursor::new(wire),
            StateStoreLimits::default(),
            SnapshotLimits::default(),
        )?;
        let (log, state) = restore_received_snapshot(
            log,
            candidate,
            self.scope.group(),
            self.scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        drop(recover_snapshotted(
            log,
            state,
            self.scope.group(),
            self.scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?);
        assert_eq!(f::state_rows(&self.root)?, f::state_rows(&child)?);
        Ok(())
    }
    pub(super) fn cleanup(self) -> TestResult {
        std::fs::remove_dir_all(self.root)?;
        Ok(())
    }
}
pub(super) fn limits() -> CreditCleanupLimits {
    CreditCleanupLimits {
        execution_workspace_bytes: 512 * f::MIB,
        ..CreditCleanupLimits::default()
    }
}

fn physical_components(bytes: &[u8]) -> TestResult<Vec<Vec<u8>>> {
    assert_eq!(bytes.get(..7), Some(b"FWPK\0\x01\x0d".as_slice()));
    let mut at = 7;
    let mut parts = Vec::new();
    for _ in 0..7 {
        let mut part = Vec::new();
        loop {
            let byte = *bytes.get(at).ok_or("truncated physical component")?;
            at += 1;
            if byte != 0 {
                part.push(byte);
                continue;
            }
            let escaped = *bytes.get(at).ok_or("truncated physical escape")?;
            at += 1;
            if escaped == 0 {
                break;
            }
            assert_eq!(escaped, 255);
            part.push(0);
        }
        parts.push(part);
    }
    assert_eq!(at, bytes.len());
    Ok(parts)
}
