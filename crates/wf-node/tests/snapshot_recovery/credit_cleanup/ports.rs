//! Configured actual reads and committed writes, with controlled lost replies.
use super::{fixtures::*, *};
use wf_domain::governance::credit::CreditOwner;
use wf_ports::{
    consensus::{
        CreditCatalogPublicationPort, MutationNotSubmitted, MutationSubmission, MutationUnknown,
    },
    log::{PreparedBytes, prepare_credit_request},
    state::{
        CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
        CreditCatalogReadPort, CreditOwnerReadPort, GovernanceCatalogPage, GovernanceCatalogRead,
        GovernanceCatalogReadPort, StateTransactionError as S,
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};

impl CreditCatalogReadPort for CleanupFixture {
    fn credit_catalog_page(
        &mut self,
        scope: &RequestScope,
        limit: u16,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, S> {
        let (_, mut state) = self.open().map_err(|_| S::Unavailable)?;
        state.credit_catalog_page(scope, limit, limits)
    }
    fn credit_catalog_owners(
        &mut self,
        scope: &RequestScope,
        entries: &[CreditCatalogEntry],
        limits: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, S> {
        let (_, mut state) = self.open().map_err(|_| S::Unavailable)?;
        state.credit_catalog_owners(scope, entries, limits)
    }
}
impl CreditOwnerReadPort for CleanupFixture {
    fn credit_catalog_owner(
        &self,
        scope: &RequestScope,
        name: &[u8],
        limits: CreditCatalogReadLimits,
    ) -> Result<Option<CreditOwner>, S> {
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.credit_catalog_owner(scope, name, limits)
    }
}
impl GovernanceCatalogReadPort for CleanupFixture {
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, S> {
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.governance_catalog_page(request, limits)
    }
    fn governance_catalog_contains(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, S> {
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.governance_catalog_contains(scope, catalog, member, limits)
    }
}
impl GovernanceCatalogProgressPort for CleanupFixture {
    fn governance_catalog_progress(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogProgress, S> {
        if self.fault.get() == CleanupFault::ReadProgress {
            return Err(S::Unavailable);
        }
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.governance_catalog_progress(scope, catalog, limits)
    }
}
impl CreditCatalogPublicationPort for CleanupFixture {
    fn prepare_catalog_command(
        &mut self,
        scope: &RequestScope,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted> {
        self.prepare_catalog_command_at(scope, 1006)
    }
    fn prepare_catalog_command_at(
        &mut self,
        scope: &RequestScope,
        floor: u64,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted> {
        if scope != &self.scope {
            return Err(MutationNotSubmitted::InvalidScope);
        }
        self.next_request += 1;
        let request = RequestId::new(format!("maintenance-{}", self.next_request).into_bytes())
            .map_err(|_| MutationNotSubmitted::Preparation)?;
        let time = if self.fault.get() == CleanupFault::StaleTime {
            floor.saturating_sub(1)
        } else {
            floor
        };
        Ok((
            request,
            f::clock(time, 0, time).map_err(|_| MutationNotSubmitted::Preparation)?,
        ))
    }
    fn submit_catalog_command(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        frame: PreparedBytes,
        workspace: usize,
    ) -> MutationSubmission {
        if scope != &self.scope || workspace < 512 * f::MIB {
            return MutationSubmission::NotSubmitted(MutationNotSubmitted::Limit);
        }
        let kind = match frame.as_bytes().get(6) {
            Some(10) => CleanupCall::Cleanup,
            Some(12) => CleanupCall::Prune,
            Some(13) => CleanupCall::Progress,
            _ => return MutationSubmission::NotSubmitted(MutationNotSubmitted::Preparation),
        };
        if kind == CleanupCall::Cleanup
            && prepare_credit_request(frame.as_bytes(), f::driver_limits().codec, 16 * f::MIB)
                .is_err()
        {
            return MutationSubmission::NotSubmitted(MutationNotSubmitted::Preparation);
        }
        self.calls.borrow_mut().push(kind);
        let fault = self.fault.get();
        if kind == CleanupCall::Prune && fault == CleanupFault::PublishOwnerBeforePrune {
            self.fault.set(CleanupFault::None);
            if let Err(error) = self.create_and_ack_before_prune() {
                self.last_error = Some(error.to_string());
                return MutationSubmission::Unknown(MutationUnknown::Unavailable);
            }
        }
        if matches!(
            (kind, fault),
            (CleanupCall::Cleanup, CleanupFault::RefuseCleanup)
                | (CleanupCall::Progress, CleanupFault::RefuseProgress)
        ) {
            self.fault.set(CleanupFault::None);
            return MutationSubmission::NotSubmitted(MutationNotSubmitted::Unavailable);
        }
        match self.commit(request, frame) {
            Ok(outcome) => {
                if matches!(
                    (kind, fault),
                    (CleanupCall::Cleanup, CleanupFault::LoseCleanup)
                ) {
                    self.fault.set(CleanupFault::None);
                    return MutationSubmission::Unknown(MutationUnknown::Unavailable);
                }
                if matches!(
                    (kind, fault),
                    (CleanupCall::Cleanup, CleanupFault::ForeignCleanup)
                        | (CleanupCall::Progress, CleanupFault::ForeignProgress)
                ) {
                    self.fault.set(CleanupFault::None);
                    return foreign_outcome(&outcome)
                        .unwrap_or(MutationSubmission::Unknown(MutationUnknown::InvalidReply));
                }
                if kind == CleanupCall::Progress && fault == CleanupFault::LoseProgress {
                    self.fault.set(CleanupFault::None);
                    MutationSubmission::Unknown(MutationUnknown::Unavailable)
                } else {
                    MutationSubmission::Retained(outcome)
                }
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
                MutationSubmission::Unknown(MutationUnknown::CommitUncertain)
            }
        }
    }
}

fn foreign_outcome(
    outcome: &wf_domain::workflow::request::RetainedOutcome,
) -> TestResult<MutationSubmission> {
    use wf_domain::workflow::request::{RetainedMetadata, RetainedOutcome};
    let scope = RequestScope::new(
        wf_types::identity::TenantId::new(b"foreign".to_vec())?,
        outcome.scope().group().clone(),
        outcome.scope().recovery().clone(),
    );
    let metadata = RetainedMetadata {
        scope,
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
            outcome.response().to_vec(),
            f::driver_limits().codec.admission(),
        )?,
    )))
}
