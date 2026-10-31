use super::*;
use std::cell::RefCell;
use wf_domain::{governance::credit::CreditOwner, workflow::request::RequestScope};
use wf_ports::{
    consensus::MutationSubmission,
    log::PreparedBytes,
    state::{
        CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
        CreditCatalogReadPort, CreditOwnerReadPort, GovernanceCatalogPage,
        GovernanceCatalogProgress, GovernanceCatalogProgressPort, GovernanceCatalogRead,
        GovernanceCatalogReadLimits, GovernanceCatalogReadPort, StateTransactionError as S,
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BudgetCall {
    ReadProgress,
    Page,
    Owner,
    Prepare,
    Submit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BudgetFault {
    None,
    UnavailableProgress,
    MalformedPage,
    MissingOwner,
    MissingRevision,
    EmptyWithRevision,
}
pub(super) struct BudgetFixture {
    pub(super) scope: RequestScope,
    pub(super) calls: RefCell<Vec<BudgetCall>>,
    pub(super) fault: BudgetFault,
    next: u64,
}
impl BudgetFixture {
    pub(super) fn new() -> TestResult<Self> {
        Ok(Self {
            scope: RequestScope::new(
                TenantId::new(b"t".to_vec())?,
                GroupId::new(b"g".to_vec())?,
                RecoveryId::new(b"r".to_vec())?,
            ),
            calls: RefCell::new(Vec::new()),
            fault: BudgetFault::None,
            next: 0,
        })
    }
}
impl CreditCatalogPublicationPort for BudgetFixture {
    fn prepare_catalog_command(
        &mut self,
        _scope: &RequestScope,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted> {
        self.calls.borrow_mut().push(BudgetCall::Prepare);
        self.next += 1;
        Ok((
            RequestId::new(format!("prepare-{}", self.next).into_bytes())
                .map_err(|_| MutationNotSubmitted::Preparation)?,
            CommittedTime::new(
                HlcTimestamp::new(1006, 0).map_err(|_| MutationNotSubmitted::Preparation)?,
                1006,
            )
            .map_err(|_| MutationNotSubmitted::Preparation)?,
        ))
    }
    fn submit_catalog_command(
        &mut self,
        _scope: &RequestScope,
        _request: &RequestId,
        _frame: PreparedBytes,
        _workspace: usize,
    ) -> MutationSubmission {
        self.calls.borrow_mut().push(BudgetCall::Submit);
        MutationSubmission::NotSubmitted(MutationNotSubmitted::Preparation)
    }
}
impl CreditCatalogReadPort for BudgetFixture {
    fn credit_catalog_page(
        &mut self,
        _scope: &RequestScope,
        _limit: u16,
        _limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, S> {
        Err(S::Unavailable)
    }
    fn credit_catalog_owners(
        &mut self,
        _scope: &RequestScope,
        _entries: &[CreditCatalogEntry],
        _limits: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, S> {
        Err(S::Unavailable)
    }
}
impl CreditOwnerReadPort for BudgetFixture {
    fn credit_catalog_owner(
        &self,
        _scope: &RequestScope,
        _name: &[u8],
        _limits: CreditCatalogReadLimits,
    ) -> Result<Option<CreditOwner>, S> {
        self.calls.borrow_mut().push(BudgetCall::Owner);
        Ok(None)
    }
}
impl GovernanceCatalogReadPort for BudgetFixture {
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        _limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, S> {
        self.calls.borrow_mut().push(BudgetCall::Page);
        if request.cursor.is_some() {
            return Ok(GovernanceCatalogPage {
                members: Vec::new(),
                full: false,
            });
        }
        Ok(GovernanceCatalogPage {
            members: if self.fault == BudgetFault::MalformedPage {
                vec![b"a".to_vec(), b"b".to_vec()]
            } else if matches!(
                self.fault,
                BudgetFault::MissingOwner | BudgetFault::MissingRevision
            ) {
                let encoded = wf_ports::state::encode_credit_catalog_members(
                    &self.scope,
                    &[b"a"],
                    65535,
                    65535,
                )
                .map_err(|_| S::InvalidChanges)?;
                vec![encoded.members().next().ok_or(S::InvalidChanges)?.to_vec()]
            } else {
                Vec::new()
            },
            full: matches!(
                self.fault,
                BudgetFault::MalformedPage
                    | BudgetFault::MissingOwner
                    | BudgetFault::MissingRevision
            ),
        })
    }
    fn governance_catalog_contains(
        &self,
        _scope: &RequestScope,
        _catalog: &[u8],
        _member: &[u8],
        _limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, S> {
        Err(S::Unavailable)
    }
}
impl GovernanceCatalogProgressPort for BudgetFixture {
    fn governance_catalog_progress(
        &self,
        _scope: &RequestScope,
        _catalog: &[u8],
        _limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogProgress, S> {
        self.calls.borrow_mut().push(BudgetCall::ReadProgress);
        if self.fault == BudgetFault::UnavailableProgress {
            Err(S::Unavailable)
        } else {
            Ok(GovernanceCatalogProgress::Missing)
        }
    }
}

impl wf_ports::state::GovernanceCatalogPruneReadPort for BudgetFixture {
    fn governance_catalog_prune_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<wf_ports::state::GovernanceCatalogPrunePage, S> {
        Ok(wf_ports::state::GovernanceCatalogPrunePage {
            page: self.governance_catalog_page(request, limits)?,
            revision: matches!(
                self.fault,
                BudgetFault::MissingOwner | BudgetFault::EmptyWithRevision
            )
            .then(|| {
                wf_domain::workflow::control::EntryIdentity::new(
                    wf_types::version::LogIdentity::new(
                        1,
                        1,
                        wf_types::version::AppliedPosition::new(0),
                    ),
                    wf_domain::workflow::control::PreparedDigest::new([1; 32]),
                )
            }),
        })
    }
}
