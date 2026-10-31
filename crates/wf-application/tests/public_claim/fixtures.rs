use super::*;
#[derive(Clone, Copy)]
pub(super) enum ClaimPortMode {
    Fresh,
    Retained,
    Unknown,
    Denied,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClaimTrace {
    Lookup,
    Prepare,
    Submit,
}
pub(super) struct ClaimSubmitFixture {
    pub mode: ClaimPortMode,
    pub trace: Vec<ClaimTrace>,
    pub submitted: Vec<Arc<PreparedBytes>>,
    pub retained: Option<Box<RetainedOutcome>>,
    pub reply: Option<M>,
    pub intent: Option<ClientIntent>,
}
impl ClaimSubmitFixture {
    pub fn new(mode: ClaimPortMode) -> Self {
        Self {
            mode,
            trace: Vec::new(),
            submitted: Vec::new(),
            retained: None,
            reply: None,
            intent: None,
        }
    }
}
impl PublicClaimSubmitPort for ClaimSubmitFixture {
    fn lookup_public_claim(
        &mut self,
        _scope: &RequestScope,
        _request: &RequestId,
        intent: ClientIntent,
        _limits: PublicClaimReadLimits,
    ) -> Result<L, N> {
        self.trace.push(ClaimTrace::Lookup);
        self.intent = Some(intent);
        match self.mode {
            ClaimPortMode::Fresh => Ok(L::Fresh),
            ClaimPortMode::Retained => self
                .retained
                .clone()
                .map(|o| L::Resolved(M::Retained(o)))
                .ok_or(N::Preparation),
            ClaimPortMode::Unknown => Ok(L::Resolved(M::Unknown(U::Unavailable))),
            ClaimPortMode::Denied => Err(N::Unauthorized),
        }
    }
    fn prepare_public_claim(
        &mut self,
        _scope: &RequestScope,
        _request: &RequestId,
        _caller: &NormalizedClaimDue<'_>,
    ) -> Result<(CommittedTime, [u8; 16]), N> {
        self.trace.push(ClaimTrace::Prepare);
        Ok((
            CommittedTime::new(
                HlcTimestamp::new(3000, 0).map_err(|_| N::Preparation)?,
                3000,
            )
            .map_err(|_| N::Preparation)?,
            [0x7a; 16],
        ))
    }
    fn submit_public_claim(
        &mut self,
        _scope: &RequestScope,
        _request: &RequestId,
        frame: Arc<PreparedBytes>,
        _workspace: usize,
    ) -> M {
        self.trace.push(ClaimTrace::Submit);
        self.submitted.push(frame);
        self.reply.take().unwrap_or(M::Unknown(U::CommitUncertain))
    }
}
pub(super) fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}
pub(super) fn id() -> TestResult<RequestId> {
    Ok(RequestId::new(b"public-original".to_vec())?)
}
pub(super) fn caller() -> TestResult<NormalizedClaimDue<'static>> {
    let mut input = ClaimDueInput::new(b"r", b"worker");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::One(b"p");
    input.limit = Some(2);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(false);
    input.requested_now_ms = Some(999_999);
    Ok(normalize_claim_due(input, ClaimCallerLimits::default())?)
}
pub(super) fn outcome(
    scope: &RequestScope,
    id: &RequestId,
    caller: &NormalizedClaimDue<'_>,
) -> TestResult<Box<RetainedOutcome>> {
    let intent = prepare_public_claim_intent(scope.tenant(), caller, 1_048_576)?;
    let body = wf_ports::log::encode_public_claim_funding_reply(
        wf_domain::governance::identified::IdentifiedCreditRejection::NotFound,
        true,
        CodecLimits::default().admission(),
        1_048_576,
    )?;
    Ok(Box::new(RetainedOutcome::new(
        RetainedMetadata {
            scope: scope.clone(),
            request: id.clone(),
            intent: ClientIntent::public_claim(*intent.digest()),
            original_position: LogIdentity::new(1, 1, AppliedPosition::new(2)),
            original_time: TimestampMillis::new(1000),
            expires_at: TimestampMillis::new(9000),
            status: RetainedStatus::Accepted,
        },
        18,
        body,
        CodecLimits::default().admission(),
    )?))
}
