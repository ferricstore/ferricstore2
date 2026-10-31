use super::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClaimApplicationFault {
    None,
    BeforeCommit,
    LoseReply,
    Race,
    DenyLookup,
    LookupUnknown,
    RefuseSubmit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClaimApplicationTrace {
    Lookup,
    Prepare,
    Submit,
    Commit,
}
pub(super) struct ClaimApplicationFixture {
    pub log: RedbLogStore,
    pub state: RedbStateStore,
    pub scope: RequestScope,
    pub fault: ClaimApplicationFault,
    pub trace: Vec<ClaimApplicationTrace>,
    pub frames: Vec<Arc<PreparedBytes>>,
    pub pending: Option<(u64, Arc<PreparedBytes>)>,
    pub next: u64,
}
impl ClaimApplicationFixture {
    pub fn new(dir: &std::path::Path) -> TestResult<Self> {
        let (mut log, mut state) = p::pair(dir, true)?;
        let mut frames = q::all()?;
        frames.truncate(3);
        for (at, frame) in frames.iter().enumerate() {
            f::append(&mut log, u64::try_from(at)?, frame.as_bytes())?;
        }
        log.save_committed(f::at(2))?;
        p::apply(&mut state, &frames, 3)?;
        Ok(Self {
            log,
            state,
            scope: f::scope()?,
            fault: ClaimApplicationFault::None,
            trace: Vec::new(),
            frames: Vec::new(),
            pending: None,
            next: 3,
        })
    }
    pub fn commit(
        &mut self,
        index: u64,
        frame: &PreparedBytes,
    ) -> TestResult<Box<RetainedOutcome>> {
        self.log.save_committed(f::at(index))?;
        let result = wf_storage::transaction::apply_public_claim_transaction(
            &mut self.state,
            &self.scope,
            frame.as_bytes(),
            f::at(index),
            native_limits()?,
        )?;
        let wf_storage::transaction::ClaimTransactionOutcome::Retained { outcome, .. } = result
        else {
            return Err("actual original Claim outcome".into());
        };
        self.trace.push(ClaimApplicationTrace::Commit);
        Ok(outcome)
    }
}
pub(super) fn native_limits() -> TestResult<wf_storage::transaction::PublicClaimTransactionLimits> {
    use wf_domain::{
        claim::{apply::ClaimWorkLimits, coordination::ClaimCoordinationLimits},
        governance::credit::CreditLimits,
    };
    Ok(wf_storage::transaction::PublicClaimTransactionLimits {
        base: wf_storage::transaction::AtomicClaimTransactionLimits {
            values: p::values()?,
            work: ClaimWorkLimits::default(),
            caller: ClaimCallerLimits::default(),
            coordination: ClaimCoordinationLimits::default(),
            credit: CreditLimits::new(4096, 1000, 8 * f::MIB, 128 * u64::try_from(f::MIB)?)?,
        },
        maximum_credit_work: 128 * u64::try_from(f::MIB)?,
        maximum_credit_temporary_bytes: 32 * f::MIB,
    })
}
pub(super) fn caller() -> TestResult<NormalizedClaimDue<'static>> {
    let mut input = ClaimDueInput::new(b"r", b"w");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::One(b"p");
    input.limit = Some(3);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(false);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    Ok(normalize_claim_due(input, ClaimCallerLimits::default())?)
}
pub(super) fn id() -> TestResult<RequestId> {
    Ok(RequestId::new(b"app-public-original".to_vec())?)
}
pub(super) fn owner(
    port: &ClaimApplicationFixture,
) -> TestResult<wf_domain::governance::credit::CreditOwner> {
    port.state
        .read_credit_owner(
            &port.scope,
            b"global",
            native_limits()?.base.credit,
            10000,
            64 * f::MIB,
        )?
        .ok_or("actual credit owner".into())
}
