use super::{fixtures::*, *};
impl PublicClaimSubmitPort for ClaimApplicationFixture {
    fn lookup_public_claim(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        intent: ClientIntent,
        limits: PublicClaimReadLimits,
    ) -> Result<Lookup, N> {
        self.trace.push(ClaimApplicationTrace::Lookup);
        if scope != &self.scope {
            return Err(N::InvalidScope);
        }
        if self.fault == ClaimApplicationFault::DenyLookup {
            return Err(N::Unauthorized);
        }
        if self.fault == ClaimApplicationFault::LookupUnknown {
            return Ok(Lookup::Resolved(M::Unknown(U::Unavailable)));
        }
        let committed = self.log.state().map_err(|_| N::Unavailable)?.committed();
        let tx = self
            .state
            .begin_apply(
                scope.group(),
                scope.recovery(),
                limits.codec.admission(),
                limits.maximum_steps,
                limits.maximum_bytes,
                limits.maximum_temporary_bytes,
            )
            .map_err(|_| N::Unavailable)?;
        let control = tx.control().map_err(|_| N::Unavailable)?;
        if committed.is_some_and(|at| at.index() > control.position()) {
            return Err(N::Unavailable);
        }
        let result = match tx
            .outcome(scope.tenant(), request)
            .map_err(|_| N::Unavailable)?
        {
            None => Lookup::Fresh,
            Some(outcome) if outcome.intent() != intent => Lookup::Resolved(M::IntentConflict),
            Some(outcome) if outcome.expires_at() <= control.time().wall() => {
                Lookup::Resolved(M::Expired)
            }
            Some(outcome) => Lookup::Resolved(M::Retained(Box::new(outcome))),
        };
        Ok(result)
    }
    fn prepare_public_claim(
        &mut self,
        scope: &RequestScope,
        _request: &RequestId,
        _caller: &NormalizedClaimDue<'_>,
    ) -> Result<(CommittedTime, [u8; 16]), N> {
        self.trace.push(ClaimApplicationTrace::Prepare);
        if scope != &self.scope {
            return Err(N::InvalidScope);
        }
        if self.fault == ClaimApplicationFault::DenyLookup {
            return Err(N::Unauthorized);
        }
        Ok((
            f::clock(104, 0, 104).map_err(|_| N::Preparation)?,
            [0x7a; 16],
        ))
    }
    fn submit_public_claim(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        frame: Arc<PreparedBytes>,
        workspace: usize,
    ) -> M {
        self.trace.push(ClaimApplicationTrace::Submit);
        self.frames.push(Arc::clone(&frame));
        if scope != &self.scope {
            return M::NotSubmitted(N::InvalidScope);
        }
        if self.fault == ClaimApplicationFault::DenyLookup {
            return M::NotSubmitted(N::Unauthorized);
        }
        if self.fault == ClaimApplicationFault::RefuseSubmit {
            return M::NotSubmitted(N::Redirect);
        }
        if workspace < f::driver_limits().workspace_bytes {
            return M::NotSubmitted(N::Limit);
        }
        let Ok(parsed) = wf_ports::log::prepare_public_claim_request(
            frame.as_bytes(),
            f::driver_limits().codec,
            ClaimCallerLimits::default(),
            32 * f::MIB,
        ) else {
            return M::NotSubmitted(N::Preparation);
        };
        if parsed.prepared().scope() != scope || parsed.prepared().request() != request {
            return M::NotSubmitted(N::InvalidScope);
        }
        if self.fault == ClaimApplicationFault::Race {
            let Ok(caller) = parsed.prepared().caller(ClaimCallerLimits::default()) else {
                return M::NotSubmitted(N::Preparation);
            };
            let winner = wf_ports::log::encode_public_claim_request(
                scope,
                request,
                match f::clock(103, 0, 103) {
                    Ok(c) => c,
                    Err(_) => return M::NotSubmitted(N::Preparation),
                },
                &caller,
                wf_ports::log::PublicClaimPreparation {
                    token_version: 1,
                    first_ordinal: 0,
                    scan_budget: 1000,
                    head_budget: 1000,
                    batch: [0x33; 16],
                },
                f::driver_limits().codec,
                32 * f::MIB,
            );
            let winner = match winner {
                Ok(w) => Arc::new(w),
                Err(_) => return M::NotSubmitted(N::Preparation),
            };
            self.fault = ClaimApplicationFault::None;
            return self.propose(winner);
        }
        self.propose(frame)
    }
}

impl ClaimApplicationFixture {
    fn propose(&mut self, frame: Arc<PreparedBytes>) -> M {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, pending)| pending.as_bytes() != frame.as_bytes())
        {
            return M::Unknown(U::InvalidReply);
        }
        if self.pending.is_none() {
            let at = self.next;
            if f::append(&mut self.log, at, frame.as_bytes()).is_err() {
                return M::Unknown(U::Unavailable);
            }
            self.next = match self.next.checked_add(1) {
                Some(n) => n,
                None => return M::Unknown(U::Unavailable),
            };
            self.pending = Some((at, frame));
        }
        if self.fault == ClaimApplicationFault::BeforeCommit {
            self.fault = ClaimApplicationFault::None;
            return M::Unknown(U::CommitUncertain);
        }
        let Some((at, frame)) = self.pending.take() else {
            return M::Unknown(U::InvalidReply);
        };
        match self.commit(at, &frame) {
            Ok(outcome) => {
                if self.fault == ClaimApplicationFault::LoseReply {
                    self.fault = ClaimApplicationFault::None;
                    M::Unknown(U::Unavailable)
                } else {
                    M::Retained(outcome)
                }
            }
            Err(_) => M::Unknown(U::CommitUncertain),
        }
    }
}
