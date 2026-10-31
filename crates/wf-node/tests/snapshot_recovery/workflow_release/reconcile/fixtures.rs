//! Real paid terminal records and real log/recovery submission; faults alter delivery.
use super::*;
use std::{path::PathBuf, sync::Arc};
use wf_domain::{
    governance::credit::CreditLimits,
    workflow::request::{RequestScope, RetainedOutcome},
};
use wf_ports::{
    log::{PreparedBytes, RaftLogStore},
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};
use wf_types::identity::RequestId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReleaseReconcileFault {
    None,
    RecordRead,
    RecordCardinality,
    RecordCorrupt,
    PublicUnknown,
    PublicRefuse,
    MarkUnknown,
    AckUnknown,
    WrongMaintenanceReply,
    OldStamp,
    Route,
    PageUnavailable(usize),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ReleaseReconcileTrace {
    Page(usize, Option<u64>),
    Records(Option<Vec<u8>>, usize),
    Credit(Vec<Vec<u8>>),
    Mark(Vec<u64>),
    Ack(u64, u64),
}
pub(super) struct ReleaseReconcileFixture {
    pub root: PathBuf,
    pub sources: Vec<RequestScope>,
    pub now: u64,
    pub next: u64,
    pub fault: ReleaseReconcileFault,
    pub trace: Vec<ReleaseReconcileTrace>,
    pub frames: Vec<Arc<PreparedBytes>>,
    pub last_error: Option<String>,
}
impl ReleaseReconcileFixture {
    // Source-style test-only queue replacement. It creates no production binding.
    pub fn seed_queue(
        &self,
        index: usize,
        intents: &[wf_domain::governance::release::ReleaseIntentRef<'_>],
        completed: &[u64],
    ) -> TestResult {
        let scope = &self.sources[index];
        seed_queue(&self.path(scope)?, scope, intents, completed)
    }
    pub fn terminal_intent(&self) -> TestResult<Vec<u8>> {
        let scope = &self.sources[0];
        let (log, state) = self.open(scope)?;
        let page = state.read_workflow_release_outbox(scope, None, 256, 10000, 16 * f::MIB)?;
        let out = page.entries.first().ok_or("intent")?.encoded().to_vec();
        drop((log, state));
        Ok(out)
    }
    pub fn new() -> TestResult<Self> {
        Self::with_prefix(6)
    }
    pub fn running() -> TestResult<Self> {
        Self::with_prefix(5)
    }
    fn with_prefix(end: usize) -> TestResult<Self> {
        let root = f::directory("automatic-release")?;
        let dir = root.join("0");
        std::fs::create_dir(&dir)?;
        let (mut log, mut state) = p::pair(&dir, true)?;
        let frames = q::all()?;
        for (index, frame) in frames.iter().take(end).enumerate() {
            f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
        }
        log.save_committed(f::at(u64::try_from(end)? - 1))?;
        p::apply(&mut state, &frames, end)?;
        drop((log, state));
        Ok(Self {
            root,
            sources: vec![f::scope()?],
            now: 106,
            next: 0,
            fault: ReleaseReconcileFault::None,
            trace: vec![],
            frames: vec![],
            last_error: None,
        })
    }
    pub fn index(&self, scope: &RequestScope) -> TestResult<usize> {
        self.sources
            .iter()
            .position(|s| s == scope)
            .ok_or_else(|| "unconfigured source".into())
    }
    pub fn add_source(&mut self) -> TestResult<usize> {
        use wf_domain::governance::identified::{
            CreditConfiguration, IdentifiedCreditMutation as M, IdentifiedCreditRequest,
        };
        use wf_storage::{
            state::{StateStoreGenesis, StateStoreLimits},
            transaction::apply_credit_transaction,
        };
        let index = self.sources.len();
        let scope = f::scope_for(b"t", format!("g{index}").as_bytes(), b"z")?;
        let path = self.root.join(index.to_string());
        std::fs::create_dir(&path)?;
        let mut log = RedbLogStore::create(
            &path.join("log.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            wf_ports::log::RaftLogLimits::default(),
        )?;
        let placements = [(
            wf_types::identity::PartitionId::new(b"p".to_vec())?,
            scope.group().clone(),
        )];
        let mut state = RedbStateStore::create_governed_claim_database(
            redb::Builder::new().create(path.join("state.redb"))?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0, 0, 0)?,
                placements: &placements,
                policies: &[],
            },
            p::defaults(),
            4,
            StateStoreLimits::default(),
        )?;
        let ids = [b"a\0".to_vec(), b"b".to_vec()];
        let mutations = [
            M::Lease {
                shard_id: 0,
                amount: 3,
                ttl_ms: 1000,
            },
            M::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &ids,
            },
        ];
        for (at, mutation) in mutations.into_iter().enumerate() {
            let stamp = f::clock(104 + u64::try_from(at)?, 0, 104 + u64::try_from(at)?)?;
            let id = RequestId::new(format!("second-source-{at}").into_bytes())?;
            let configuration = (at == 0).then_some(CreditConfiguration {
                limit: 3,
                config_version: Some(1),
                policy_fingerprint: None,
            });
            let frame = wf_ports::log::encode_credit_request(
                &scope,
                &id,
                stamp,
                IdentifiedCreditRequest {
                    scope: b"global",
                    shard_count: 4,
                    configuration,
                    mutation,
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?;
            f::append(&mut log, u64::try_from(at)?, frame.as_bytes())?;
            apply_credit_transaction(
                &mut state,
                &scope,
                frame.as_bytes(),
                f::at(u64::try_from(at)?),
                f::driver_limits(),
                CreditLimits::default(),
            )?;
        }
        log.save_committed(f::at(1))?;
        drop((log, state));
        self.sources.push(scope);
        Ok(index)
    }
    pub fn receive_snapshot(&self) -> TestResult {
        use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
        let scope = &self.sources[0];
        let (log, mut state) = self.open(scope)?;
        let mut wire = Vec::new();
        state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
        drop((log, state));
        let target = self.root.join("received");
        std::fs::create_dir(&target)?;
        let log = p::log(&target)?;
        let candidate = wf_storage::snapshot::RedbSnapshotStage::receive(
            &target.join("state.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            &mut std::io::Cursor::new(wire),
            wf_storage::state::StateStoreLimits::default(),
            SnapshotLimits::default(),
        )?;
        drop(wf_node::recovery::restore_received_snapshot(
            log,
            candidate,
            scope.group(),
            scope.recovery(),
            p::selected(true)?.with_public_credit_limits(4, CreditLimits::default())?,
            SnapshotLimits::default(),
        )?);
        std::fs::rename(self.path(scope)?, self.root.join("before-receive"))?;
        std::fs::rename(target, self.path(scope)?)?;
        Ok(())
    }
    pub fn original(&self, scope: &RequestScope, id: &[u8]) -> TestResult<Box<RetainedOutcome>> {
        let (log, mut state) = self.open(scope)?;
        let tx = state.begin_apply(
            scope.group(),
            scope.recovery(),
            f::driver_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?;
        let result = tx
            .outcome(scope.tenant(), &RequestId::new(id.to_vec())?)?
            .ok_or("receipt")?;
        drop(tx);
        drop((log, state));
        Ok(Box::new(result))
    }
    pub fn reserve(&mut self, id: &[u8]) -> TestResult {
        use wf_domain::governance::identified::{
            IdentifiedCreditMutation as M, IdentifiedCreditRequest,
        };
        let scope = self.sources[0].clone();
        let (request, stamp) = self.identity(&scope).map_err(|e| format!("{e:?}"))?;
        let ids = [id.to_vec()];
        let frame = wf_ports::log::encode_credit_request(
            &scope,
            &request,
            stamp,
            IdentifiedCreditRequest {
                scope: b"global",
                shard_count: 4,
                configuration: None,
                mutation: M::Spend {
                    shard_id: 0,
                    lease_epoch: 1,
                    amount: 1,
                    ttl_ms: None,
                    reservation_ids: &ids,
                },
            },
            f::driver_limits().codec,
            16 * f::MIB,
        )?;
        assert_eq!(
            self.commit(&scope, &request, &frame)?.status(),
            wf_domain::workflow::request::RetainedStatus::Accepted
        );
        Ok(())
    }
    pub fn corrupt_first_intent(&self, index: usize) -> TestResult {
        let scope = &self.sources[index];
        let db = redb::Builder::new().open(self.path(scope)?.join("state.redb"))?;
        let tx = db.begin_write()?;
        {
            let mut table = tx.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
                "wf_state_rows_v1",
            ))?;
            table.insert(
                queue_key(scope, 22, Some(1)).as_slice(),
                [0_u8, 1, 2, 3].as_slice(),
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn path(&self, scope: &RequestScope) -> TestResult<PathBuf> {
        Ok(self.root.join(self.index(scope)?.to_string()))
    }
    pub fn open(&self, scope: &RequestScope) -> TestResult<(RedbLogStore, RedbStateStore)> {
        f::open_pair_for(&self.path(scope)?, scope)
    }
    pub fn identity(
        &mut self,
        scope: &RequestScope,
    ) -> Result<
        (RequestId, wf_types::version::CommittedTime),
        wf_ports::consensus::MutationNotSubmitted,
    > {
        use wf_ports::consensus::MutationNotSubmitted as N;
        self.index(scope).map_err(|_| N::InvalidScope)?;
        self.next += 1;
        Ok((
            RequestId::new(format!("release-reconcile-{}", self.next).into_bytes())
                .map_err(|_| N::Preparation)?,
            f::clock(self.now, 0, self.now).map_err(|_| N::Preparation)?,
        ))
    }
    pub fn commit(
        &mut self,
        scope: &RequestScope,
        id: &RequestId,
        frame: &PreparedBytes,
    ) -> TestResult<Box<RetainedOutcome>> {
        let (mut log, state) = self.open(scope)?;
        let index = log.state()?.last_log().map_or(0, |id| id.index().get() + 1);
        f::append(&mut log, index, frame.as_bytes())?;
        log.save_committed(f::at(index))?;
        let (log, mut state) = wf_node::recovery::recover_snapshotted(
            log,
            state,
            scope.group(),
            scope.recovery(),
            p::selected(true)?.with_public_credit_limits(4, CreditLimits::default())?,
            wf_ports::snapshot::SnapshotLimits::default(),
        )?;
        let tx = state.begin_apply(
            scope.group(),
            scope.recovery(),
            f::driver_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?;
        let out = tx.outcome(scope.tenant(), id)?.ok_or("original outcome")?;
        drop(tx);
        drop((log, state));
        Ok(Box::new(out))
    }
    pub fn submit(
        &mut self,
        scope: &RequestScope,
        id: &RequestId,
        frame: &PreparedBytes,
    ) -> wf_ports::consensus::MutationSubmission {
        use wf_ports::consensus::{MutationSubmission as M, MutationUnknown as U};
        match self.commit(scope, id, frame) {
            Ok(out) => M::Retained(out),
            Err(e) => {
                self.last_error = Some(e.to_string());
                M::Unknown(U::CommitUncertain)
            }
        }
    }
    pub fn assert_released(&self) -> TestResult {
        let s = &self.sources[0];
        let (log, mut state) = self.open(s)?;
        assert_eq!(
            state
                .read_workflow_release_outbox(s, None, 256, 10000, 16 * f::MIB)?
                .entries,
            Vec::<wf_ports::state::WorkflowReleaseEntry>::new()
        );
        assert_eq!(
            state
                .read_credit_owner(s, b"global", CreditLimits::default(), 10000, 16 * f::MIB)?
                .ok_or("owner")?
                .view()
                .leases[0]
                .in_use,
            1
        );
        assert_eq!(p::record(&mut state, b"j")?.record.state(), b"running");
        drop((log, state));
        let rows = f::state_rows(&self.path(s)?)?;
        let markers: Vec<_> = rows.keys().filter(|k| k.get(6) == Some(&24)).collect();
        assert_eq!(markers.len(), 1);
        assert!(markers[0].ends_with(b"b\0\0"));
        Ok(())
    }
    pub fn cleanup(self) -> TestResult {
        std::fs::remove_dir_all(self.root)?;
        Ok(())
    }
}

fn queue_key(scope: &RequestScope, family: u8, sequence: Option<u64>) -> Vec<u8> {
    let mut out = b"FWPK\0\x01".to_vec();
    out.push(family);
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
    ] {
        for byte in field {
            out.push(*byte);
            if *byte == 0 {
                out.push(255);
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    if let Some(sequence) = sequence {
        out.extend_from_slice(&sequence.to_be_bytes());
    }
    out
}

fn seed_queue(
    path: &std::path::Path,
    scope: &RequestScope,
    intents: &[wf_domain::governance::release::ReleaseIntentRef<'_>],
    completed: &[u64],
) -> TestResult {
    use redb::{ReadableTable, TableDefinition};
    use wf_domain::governance::release::ReleaseOutboxState;
    let db = redb::Builder::new().open(path.join("state.redb"))?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        let mut old = Vec::new();
        for row in table.iter()? {
            let (key, _) = row?;
            if matches!(key.value().get(6), Some(21..=23)) {
                old.push(key.value().to_vec());
            }
        }
        for key in old {
            table.remove(key.as_slice())?;
        }
        let meta = wf_ports::log::encode_release_meta(
            ReleaseOutboxState {
                head: 1,
                tail: u64::try_from(intents.len())?,
            },
            128,
        )?;
        table.insert(queue_key(scope, 21, None).as_slice(), meta.as_slice())?;
        for (at, intent) in intents.iter().enumerate() {
            let seq = u64::try_from(at)? + 1;
            let value = wf_ports::log::encode_release_intent(*intent, 1024 * 1024)?;
            table.insert(queue_key(scope, 22, Some(seq)).as_slice(), value.as_slice())?;
        }
        for seq in completed {
            table.insert(
                queue_key(scope, 23, Some(*seq)).as_slice(),
                b"FWRC\0\x01".as_slice(),
            )?;
        }
    }
    tx.commit()?;
    drop(db);
    drop(f::open_pair_for(path, scope)?);
    Ok(())
}
