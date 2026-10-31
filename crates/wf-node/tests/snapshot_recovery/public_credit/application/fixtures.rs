//! Actual GC3/credit2 Node stores with controlled admission and reply faults.
use super::*;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::Arc,
};
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_ports::{
    consensus::MutationSubmission,
    log::{PreparedBytes, RaftLogStore},
    snapshot::SnapshotLimits,
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};
use wf_types::identity::RequestId;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicCreditTrace {
    Lookup,
    Owner,
    Member,
    Maintenance,
    Register(bool),
    Ack,
    Prepare,
    Entropy,
    Submit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicCreditFault {
    None,
    RefuseRegister,
    UnknownRegister,
    LosePublic,
    WrongPublicIds,
    OwnerRead,
    LookupUnknown,
    RefusePublic,
    ConcurrentGet,
}
pub(super) struct PublicCreditFixture {
    pub(super) root: PathBuf,
    pub(super) scope: RequestScope,
    pub(super) now: u64,
    pub(super) trace: RefCell<Vec<PublicCreditTrace>>,
    pub(super) fault: Cell<PublicCreditFault>,
    pub(super) frames: Vec<Arc<PreparedBytes>>,
    pub(super) next: u64,
    pub(super) batch: u8,
    pub(super) last_error: Option<String>,
}
impl PublicCreditFixture {
    pub(super) fn new() -> TestResult<Self> {
        let root = f::directory("public-credit-app")?;
        drop(super::super::fixtures::pair(&root)?);
        Ok(Self {
            root,
            scope: f::scope()?,
            now: 1000,
            trace: RefCell::new(Vec::new()),
            fault: Cell::new(PublicCreditFault::None),
            frames: Vec::new(),
            next: 0,
            batch: 0,
            last_error: None,
        })
    }
    pub(super) fn open(&self) -> TestResult<(RedbLogStore, RedbStateStore)> {
        f::open_pair(&self.root)
    }
    pub(super) fn commit(
        &self,
        id: &RequestId,
        frame: &PreparedBytes,
    ) -> TestResult<Box<RetainedOutcome>> {
        let (mut log, state) = self.open()?;
        let index = log.state()?.last_log().map_or(0, |id| id.index().get() + 1);
        f::append(&mut log, index, frame.as_bytes())?;
        log.save_committed(f::at(index))?;
        let (log, mut state) = wf_node::recovery::recover_snapshotted(
            log,
            state,
            self.scope.group(),
            self.scope.recovery(),
            super::super::fixtures::selected()?,
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
            .outcome(self.scope.tenant(), id)?
            .ok_or("missing actual public app outcome")?;
        drop(tx);
        drop((log, state));
        Ok(Box::new(outcome))
    }
    pub(super) fn seed(&self) -> TestResult {
        let frames = super::super::fixtures::frames()?;
        self.commit(&RequestId::new(b"lease".to_vec())?, &frames[0])?;
        Ok(())
    }
    pub(super) fn call(
        &mut self,
        kind: K,
        id: &[u8],
        options: &[(&str, V<'_>)],
    ) -> TestResult<Result<R, E>> {
        let source = self.scope.clone();
        let request = RequestId::new(id.to_vec())?;
        let caller = normalize_credit_call(kind, b"global", 4, options, 16 * f::MIB)?;
        Ok(submit_public_credit_call(
            Request {
                source: &source,
                target: &source,
                catalog: b"limit",
                request: &request,
                caller,
            },
            self,
            Limits::default(),
        ))
    }
    pub(super) fn owner(&self) -> TestResult<Option<wf_domain::governance::credit::CreditOwner>> {
        let (_, state) = self.open()?;
        Ok(state.read_credit_owner(
            &self.scope,
            b"global",
            wf_domain::governance::credit::CreditLimits::default(),
            10000,
            64 * f::MIB,
        )?)
    }
    pub(super) fn member(&self) -> TestResult<bool> {
        use wf_ports::state::{GovernanceCatalogReadPort, encode_credit_catalog_members};
        let encoded = encode_credit_catalog_members(&self.scope, &[b"global"], 65535, 65535)?;
        let (_, state) = self.open()?;
        Ok(state.governance_catalog_contains(
            &self.scope,
            b"limit",
            encoded.members().next().ok_or("missing member")?,
            wf_ports::state::GovernanceCatalogReadLimits::default(),
        )?)
    }
    pub(super) fn in_use(&self) -> TestResult<u64> {
        Ok(self
            .owner()?
            .ok_or("missing owner")?
            .lease(0)
            .ok_or("missing lease")?
            .in_use)
    }
    pub(super) fn pending(&self) -> TestResult<usize> {
        let (_, state) = self.open()?;
        Ok(state
            .read_credit_catalog(&self.scope, 256, 10000, 64 * f::MIB)?
            .entries
            .len())
    }
    pub(super) fn cleanup(self) -> TestResult {
        std::fs::remove_dir_all(self.root)?;
        Ok(())
    }
}
pub(super) fn decoded(
    outcome: &RetainedOutcome,
) -> TestResult<wf_domain::governance::identified::IdentifiedCreditReply> {
    Ok(wf_ports::log::decode_credit_reply(
        outcome.response(),
        wf_domain::governance::credit::CreditLimits::default(),
        16 * f::MIB,
        32 * f::MIB,
    )?)
}
pub(super) fn corrupt_ids(outcome: &RetainedOutcome) -> TestResult<MutationSubmission> {
    use wf_domain::workflow::request::RetainedMetadata;
    let mut reply = decoded(outcome)?;
    reply.reservation_ids.reverse();
    let bytes = wf_ports::log::encode_credit_reply(&reply, 16 * f::MIB, 32 * f::MIB)?;
    let metadata = RetainedMetadata {
        scope: outcome.scope().clone(),
        request: outcome.request().clone(),
        intent: outcome.intent(),
        original_position: outcome.original_position(),
        original_time: outcome.original_time(),
        expires_at: outcome.expires_at(),
        status: outcome.status(),
    };
    Ok(MutationSubmission::Retained(Box::new(
        RetainedOutcome::new(
            metadata,
            outcome.response_version(),
            bytes,
            f::driver_limits().codec.admission(),
        )?,
    )))
}
