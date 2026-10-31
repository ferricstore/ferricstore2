use super::*;
mod observations;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use wf_application::governance::{
    CreditCatalogPublicationRequest as Request, publish_credit_catalog,
};
use wf_domain::{
    governance::{
        credit::CreditLimits,
        identified::{
            CreditConfiguration, IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
        },
    },
    workflow::{
        active::LifecycleDefaults,
        control::PreparedDigest,
        request::{RequestScope, RetainedMetadata, RetainedOutcome},
    },
};
use wf_node::recovery::{
    CommittedRecoveryLimits, purge_snapshot, recover_committed, recover_snapshotted,
    restore_received_snapshot,
};
use wf_ports::{
    consensus::{
        CreditCatalogPublicationPort, MutationNotSubmitted, MutationSubmission, MutationUnknown,
        WorkflowValueDefaults,
    },
    log::{PreparedBytes, RaftLogLimits, RaftLogStore, encode_credit_request},
    snapshot::{SnapshotLimits, StateSnapshotStore},
    state::{
        CreditCatalogEntry, CreditCatalogOwnerState as Owner, CreditCatalogPage,
        CreditCatalogReadLimits, CreditCatalogReadPort, GovernanceCatalogRead, GroupApplyStore,
        GroupApplyTransaction, StateTransactionError as S,
    },
};
use wf_storage::{
    log::RedbLogStore,
    snapshot::RedbSnapshotStage,
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::CommittedTime,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum PublicationStoreProfile {
    Source,
    Target,
    Combined,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationFault {
    None,
    LoseMembershipReply,
    LoseAckReply,
    RefuseMembership,
    RefuseAck,
    InvalidMembershipReply,
    ShortOwners,
    CorruptMiddle,
    UnavailablePage,
    CreateTailOwnerAfterRead,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationCall {
    Page,
    Owners,
    Membership,
    Ack,
}
pub(super) struct PublicationStore {
    scope: RequestScope,
    path: PathBuf,
    profile: PublicationStoreProfile,
}
pub(super) struct PublicationFixture {
    root: PathBuf,
    stores: Vec<PublicationStore>,
    pub(super) sources: Vec<RequestScope>,
    target: RequestScope,
    pub(super) calls: Vec<PublicationCall>,
    pub(super) fault: PublicationFault,
    next_request: u64,
    pub(super) last_error: Option<String>,
}

fn scope(group: &[u8]) -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(group.to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
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
fn caps(profile: PublicationStoreProfile) -> TestResult<CommittedRecoveryLimits> {
    let base = super::super::caps()?;
    Ok(match profile {
        PublicationStoreProfile::Source => {
            base.with_identified_credit_catalog_limits(4, CreditLimits::default())?
        }
        PublicationStoreProfile::Target => base.with_governance_catalogs(),
        PublicationStoreProfile::Combined => base
            .with_identified_credit_catalog_limits(4, CreditLimits::default())?
            .with_governance_catalogs(),
    })
}
fn create(path: &Path, scope: &RequestScope, profile: PublicationStoreProfile) -> TestResult {
    std::fs::create_dir(path)?;
    drop(RedbLogStore::create(
        &path.join("log.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?);
    let database = redb::Builder::new().create(path.join("state.redb"))?;
    let genesis = StateStoreGenesis {
        initial_time: f::clock(0, 0, 0)?,
        placements: &[],
        policies: &[],
    };
    let state = match profile {
        PublicationStoreProfile::Source => RedbStateStore::create_credit_catalog_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            4,
            StateStoreLimits::default(),
        )?,
        PublicationStoreProfile::Target => RedbStateStore::create_governance_catalog_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            StateStoreLimits::default(),
        )?,
        PublicationStoreProfile::Combined => {
            RedbStateStore::create_credit_and_governance_catalog_database(
                database,
                scope.group().clone(),
                scope.recovery().clone(),
                genesis,
                defaults(),
                4,
                StateStoreLimits::default(),
            )?
        }
    };
    drop(state);
    Ok(())
}
impl PublicationFixture {
    fn recreate_tail_owner(&self, source: &RequestScope) -> TestResult {
        let request = RequestId::new(b"tail-recreation".to_vec())?;
        let frame = encode_credit_request(
            source,
            &request,
            f::clock(1500, 0, 1500)?,
            IdentifiedCreditRequest {
                scope: b"c",
                shard_count: 4,
                configuration: Some(CreditConfiguration {
                    limit: 1,
                    config_version: Some(1),
                    policy_fingerprint: None,
                }),
                mutation: Op::Lease {
                    shard_id: 0,
                    amount: 1,
                    ttl_ms: 1000,
                },
            },
            f::driver_limits().codec,
            16 * f::MIB,
        )?;
        self.commit(source, &request, frame)?;
        let (_, state, _) = self.open(source)?;
        assert!(
            state
                .read_credit_owner(source, b"c", CreditLimits::default(), 10000, 64 * f::MIB)?
                .is_some()
        );
        Ok(())
    }
    pub(super) fn separate() -> TestResult<Self> {
        let root = f::directory("catalog-publisher-separate")?;
        let sources = vec![scope(b"g0")?, scope(b"g1")?];
        let target = scope(b"target")?;
        let mut stores = Vec::new();
        for (index, scope) in sources.iter().chain(std::iter::once(&target)).enumerate() {
            let path = root.join(index.to_string());
            let profile = if index == 2 {
                PublicationStoreProfile::Target
            } else {
                PublicationStoreProfile::Source
            };
            create(&path, scope, profile)?;
            stores.push(PublicationStore {
                scope: scope.clone(),
                path,
                profile,
            });
        }
        let mut this = Self {
            root,
            stores,
            sources,
            target,
            calls: Vec::new(),
            fault: PublicationFault::None,
            next_request: 0,
            last_error: None,
        };
        this.seed(1)?;
        Ok(this)
    }
    pub(super) fn combined() -> TestResult<Self> {
        let root = f::directory("catalog-publisher-combined")?;
        let source = scope(b"g")?;
        let path = root.join("0");
        create(&path, &source, PublicationStoreProfile::Combined)?;
        let mut this = Self {
            root,
            stores: vec![PublicationStore {
                scope: source.clone(),
                path,
                profile: PublicationStoreProfile::Combined,
            }],
            sources: vec![source.clone()],
            target: source,
            calls: Vec::new(),
            fault: PublicationFault::None,
            next_request: 0,
            last_error: None,
        };
        this.seed(0)?;
        Ok(this)
    }
    fn find(&self, scope: &RequestScope) -> Result<&PublicationStore, S> {
        self.stores
            .iter()
            .find(|store| &store.scope == scope)
            .ok_or(S::NamespaceConflict)
    }
    fn open(
        &self,
        scope: &RequestScope,
    ) -> TestResult<(RedbLogStore, RedbStateStore, PublicationStoreProfile)> {
        let item = self.find(scope)?;
        Ok((
            RedbLogStore::open(
                &item.path.join("log.redb"),
                scope.group().clone(),
                scope.recovery().clone(),
                RaftLogLimits::default(),
            )?,
            RedbStateStore::open(
                &item.path.join("state.redb"),
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?,
            item.profile,
        ))
    }
    fn seed(&mut self, index: usize) -> TestResult {
        let source = self.sources[index].clone();
        for (index, name) in [b"a", b"b", b"c"].into_iter().enumerate() {
            let request = RequestId::new([b"owner-".as_slice(), name].concat())?;
            let stamp = 1000 + u64::try_from(index)?;
            let frame = encode_credit_request(
                &source,
                &request,
                f::clock(stamp, 0, stamp)?,
                IdentifiedCreditRequest {
                    scope: name,
                    shard_count: 4,
                    configuration: Some(CreditConfiguration {
                        limit: 1,
                        config_version: Some(1),
                        policy_fingerprint: None,
                    }),
                    mutation: Op::Lease {
                        shard_id: 0,
                        amount: 1,
                        ttl_ms: 1000,
                    },
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?;
            self.commit(&source, &request, frame)?;
        }
        Ok(())
    }
    fn commit(
        &self,
        scope: &RequestScope,
        request: &RequestId,
        frame: PreparedBytes,
    ) -> TestResult<Box<RetainedOutcome>> {
        let (mut log, state, profile) = self.open(scope)?;
        let index = log.state()?.last_log().map_or(0, |id| id.index().get() + 1);
        f::append(&mut log, index, frame.as_bytes())?;
        log.save_committed(f::at(index))?;
        drop(frame);
        let (log, mut state) =
            recover_committed(log, state, scope.group(), scope.recovery(), caps(profile)?)?;
        let tx = state.begin_apply(
            scope.group(),
            scope.recovery(),
            f::driver_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?;
        let original = tx
            .outcome(scope.tenant(), request)?
            .ok_or("committed publisher outcome absent")?;
        drop(tx);
        drop((log, state));
        Ok(Box::new(original))
    }
    pub(super) fn run(&mut self, cursor: u64, mut limits: Limits) -> TestResult<Stats> {
        limits.execution_workspace_bytes = 512 * f::MIB;
        let sources = self.sources.clone();
        let target = self.target.clone();
        Ok(publish_credit_catalog(
            Request {
                sources: &sources,
                source_cursor: cursor,
                target: &target,
                catalog: b"limit",
            },
            self,
            limits,
        )?)
    }
    pub(super) fn members(&self) -> TestResult<Vec<Vec<u8>>> {
        let (_, state, _) = self.open(&self.target)?;
        Ok(state
            .read_governance_catalog(
                GovernanceCatalogRead {
                    scope: &self.target,
                    catalog: b"limit",
                    cursor: None,
                    limit: 256,
                },
                10000,
                64 * f::MIB,
            )?
            .members)
    }
    pub(super) fn pending(&self, index: usize) -> TestResult<CreditCatalogPage> {
        let (_, state, _) = self.open(&self.sources[index])?;
        Ok(state.read_credit_catalog(&self.sources[index], 256, 10000, 64 * f::MIB)?)
    }
}

impl CreditCatalogReadPort for PublicationFixture {
    fn credit_catalog_page(
        &mut self,
        scope: &RequestScope,
        limit: u16,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, S> {
        self.calls.push(PublicationCall::Page);
        if self.fault == PublicationFault::UnavailablePage {
            self.fault = PublicationFault::None;
            return Err(S::Unavailable);
        }
        let (_, mut state, _) = self.open(scope).map_err(|_| S::Unavailable)?;
        state.credit_catalog_page(scope, limit, limits)
    }
    fn credit_catalog_owners(
        &mut self,
        scope: &RequestScope,
        entries: &[CreditCatalogEntry],
        limits: CreditCatalogReadLimits,
    ) -> Result<Vec<Owner>, S> {
        self.calls.push(PublicationCall::Owners);
        let (_, mut state, _) = self.open(scope).map_err(|_| S::Unavailable)?;
        let mut result = state.credit_catalog_owners(scope, entries, limits)?;
        drop(state);
        if self.fault == PublicationFault::CreateTailOwnerAfterRead {
            self.fault = PublicationFault::None;
            if let Err(error) = self.recreate_tail_owner(scope) {
                self.last_error = Some(error.to_string());
                return Err(S::Unavailable);
            }
        }
        if self.fault == PublicationFault::ShortOwners {
            self.fault = PublicationFault::None;
            result.pop();
        } else if self.fault == PublicationFault::CorruptMiddle {
            self.fault = PublicationFault::None;
            let status = result.get_mut(1).ok_or(S::Corrupt)?;
            *status = Owner::Corrupt;
        }
        Ok(result)
    }
}
impl CreditCatalogPublicationPort for PublicationFixture {
    fn prepare_catalog_command(
        &mut self,
        scope: &RequestScope,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted> {
        self.find(scope)
            .map_err(|_| MutationNotSubmitted::InvalidScope)?;
        self.next_request = self
            .next_request
            .checked_add(1)
            .ok_or(MutationNotSubmitted::Limit)?;
        let request = RequestId::new(format!("catalog-job-{}", self.next_request).into_bytes())
            .map_err(|_| MutationNotSubmitted::Preparation)?;
        let time = 2000 + self.next_request;
        let stamp = f::clock(time, 0, time).map_err(|_| MutationNotSubmitted::Preparation)?;
        Ok((request, stamp))
    }
    fn submit_catalog_command(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        prepared: PreparedBytes,
        workspace: usize,
    ) -> MutationSubmission {
        if workspace < 512 * f::MIB {
            return MutationSubmission::NotSubmitted(MutationNotSubmitted::Limit);
        }
        let membership = prepared.as_bytes().get(6) == Some(&12);
        self.calls.push(if membership {
            PublicationCall::Membership
        } else {
            PublicationCall::Ack
        });
        let fault = match self.fault {
            PublicationFault::RefuseMembership
            | PublicationFault::LoseMembershipReply
            | PublicationFault::InvalidMembershipReply
                if membership =>
            {
                std::mem::replace(&mut self.fault, PublicationFault::None)
            }
            PublicationFault::RefuseAck | PublicationFault::LoseAckReply if !membership => {
                std::mem::replace(&mut self.fault, PublicationFault::None)
            }
            _ => PublicationFault::None,
        };
        if matches!(
            fault,
            PublicationFault::RefuseMembership | PublicationFault::RefuseAck
        ) {
            return MutationSubmission::NotSubmitted(MutationNotSubmitted::Unavailable);
        }
        let mut outcome = match self.commit(scope, request, prepared) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.last_error = Some(error.to_string());
                return MutationSubmission::Unknown(MutationUnknown::Unavailable);
            }
        };
        if matches!(
            fault,
            PublicationFault::LoseMembershipReply | PublicationFault::LoseAckReply
        ) {
            return MutationSubmission::Unknown(MutationUnknown::Unavailable);
        }
        if fault == PublicationFault::InvalidMembershipReply {
            let metadata = RetainedMetadata {
                scope: RequestScope::new(
                    TenantId::new(b"foreign".to_vec()).unwrap_or_else(|_| scope.tenant().clone()),
                    scope.group().clone(),
                    scope.recovery().clone(),
                ),
                request: outcome.request().clone(),
                intent: outcome.intent(),
                original_position: outcome.original_position(),
                original_time: outcome.original_time(),
                expires_at: outcome.expires_at(),
                status: outcome.status(),
            };
            match RetainedOutcome::new(
                metadata,
                outcome.response_version(),
                outcome.response().to_vec(),
                f::driver_limits().codec.admission(),
            ) {
                Ok(altered) => outcome = Box::new(altered),
                Err(error) => {
                    self.last_error = Some(error.to_string());
                    return MutationSubmission::Unknown(MutationUnknown::InvalidReply);
                }
            }
        }
        MutationSubmission::Retained(outcome)
    }
}
