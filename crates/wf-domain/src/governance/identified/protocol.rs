//! Consumed phased state and sealed sparse plans, without runtime authority.

use super::{
    CreditCleanupChange, CreditCleanupTask, CreditPageChange, CreditProtocolObservation,
    CreditProtocolQuery, CreditReservationChange, IdentifiedCreditDecision as D,
    IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
};
use crate::governance::catalog::CreditCatalogMeta;
use crate::governance::credit::{
    CreditError as E, CreditLimits, CreditOwner, CreditOwnerData,
    admission::{CreditCharge, product},
    restore_credit_owner,
};
use wf_types::version::TimestampMillis;

#[derive(Clone, Copy)]
pub(super) enum CreditPhase {
    Complete,
    Reservations {
        shard_id: u64,
        epoch: u64,
    },
    SpendPage {
        shard_id: u64,
        epoch: u64,
        page: u16,
    },
    CleanupTask {
        sequence: u64,
    },
    CleanupPage {
        task: CreditCleanupTask,
    },
}

/// Consumed mathematical preparation carrying one continuous resource account.
pub struct PreparedIdentifiedCredit<'a> {
    pub(super) data: Option<CreditOwnerData>,
    pub(super) request: IdentifiedCreditRequest<'a>,
    pub(super) now: TimestampMillis,
    pub(super) limits: CreditLimits,
    pub(super) charge: CreditCharge,
    pub(super) phase: CreditPhase,
    pub(super) decision: D,
    pub(super) publish_owner: bool,
    pub(super) catalog_publication: Option<CreditCatalogMeta>,
    pub(super) reservations: Vec<CreditReservationChange<'a>>,
    pub(super) pages: Vec<CreditPageChange>,
    pub(super) cleanup: Vec<CreditCleanupChange>,
}

impl<'a> PreparedIdentifiedCredit<'a> {
    /// Borrows exact original IDs; no later phase may replace or regenerate them.
    #[must_use]
    pub fn original_ids(&self) -> &'a [Vec<u8>] {
        match self.request.mutation {
            Op::Spend {
                reservation_ids, ..
            }
            | Op::Release {
                reservation_ids, ..
            } => reservation_ids,
            _ => &[],
        }
    }

    /// Returns only the exact next required read, or actual phase completion.
    #[must_use]
    pub fn query(&self) -> Option<CreditProtocolQuery<'a>> {
        Some(match self.phase {
            CreditPhase::Complete => return None,
            CreditPhase::Reservations { shard_id, epoch } => CreditProtocolQuery::Reservations {
                shard_id,
                epoch,
                ids: self.original_ids(),
            },
            CreditPhase::SpendPage {
                shard_id,
                epoch,
                page,
            } => CreditProtocolQuery::Page {
                shard_id,
                epoch,
                page,
            },
            CreditPhase::CleanupTask { sequence } => CreditProtocolQuery::CleanupTask { sequence },
            CreditPhase::CleanupPage { task } => CreditProtocolQuery::Page {
                shard_id: task.shard_id,
                epoch: task.epoch,
                page: task.next_page,
            },
        })
    }

    /// Seals only a fully completed decision and all its sparse effects.
    /// # Errors
    /// Refuses an incomplete phase, canonical state or continuous resource failure.
    pub fn finish(mut self) -> Result<IdentifiedCreditPlan<'a>, E> {
        if !matches!(self.phase, CreditPhase::Complete) {
            return Err(E::InvalidInput);
        }
        let owner = if let Some(data) = self.data {
            self.charge.work(
                4096_usize
                    .checked_add(data.scope.len())
                    .and_then(|n| {
                        product(data.leases.len(), 256)
                            .ok()
                            .and_then(|cost| n.checked_add(cost))
                    })
                    .ok_or(E::Limit)?,
            )?;
            Some(restore_credit_owner(data, self.limits)?)
        } else {
            None
        };
        let response_ids = if matches!(self.decision, D::Spent { .. }) {
            match self.request.mutation {
                Op::Spend {
                    reservation_ids, ..
                } => reservation_ids,
                _ => return Err(E::InvalidState),
            }
        } else {
            &[]
        };
        Ok(IdentifiedCreditPlan {
            work_units: self.charge.used_work(),
            scope: self.request.scope,
            owner,
            decision: self.decision,
            publish_owner: self.publish_owner,
            catalog_publication: self.catalog_publication,
            response_ids,
            reservations: self.reservations,
            pages: self.pages,
            cleanup: self.cleanup,
        })
    }
}

/// Complete mathematical effects; only a real adapter can publish them.
pub struct IdentifiedCreditPlan<'a> {
    work_units: u64,
    scope: &'a [u8],
    owner: Option<CreditOwner>,
    decision: D,
    publish_owner: bool,
    catalog_publication: Option<CreditCatalogMeta>,
    response_ids: &'a [Vec<u8>],
    reservations: Vec<CreditReservationChange<'a>>,
    pages: Vec<CreditPageChange>,
    cleanup: Vec<CreditCleanupChange>,
}

impl IdentifiedCreditPlan<'_> {
    /// Actual represented work consumed by this complete original phased plan.
    #[must_use]
    pub const fn work_units(&self) -> u64 {
        self.work_units
    }
    /// Proposed publication header; neither current-state nor commit authority.
    #[must_use]
    pub const fn catalog_publication(&self) -> Option<CreditCatalogMeta> {
        self.catalog_publication
    }

    /// Exact original governance scope.
    #[must_use]
    pub const fn scope(&self) -> &[u8] {
        self.scope
    }
    /// Original represented response owner; no CURRENT ownership proof.
    #[must_use]
    pub const fn owner(&self) -> Option<&CreditOwner> {
        self.owner.as_ref()
    }
    /// Owner replacement only when the complete plan publishes source effects.
    #[must_use]
    pub fn changed_owner(&self) -> Option<&CreditOwner> {
        self.publish_owner.then_some(self.owner.as_ref()).flatten()
    }
    /// Original deterministic classification.
    #[must_use]
    pub const fn decision(&self) -> D {
        self.decision
    }
    /// Actual original spent IDs, in their original order.
    #[must_use]
    pub const fn response_ids(&self) -> &[Vec<u8>] {
        self.response_ids
    }
    /// Exact identified point effects.
    #[must_use]
    pub fn reservations(&self) -> &[CreditReservationChange<'_>] {
        &self.reservations
    }
    /// Exact page effects, preserving page ID insertion order.
    #[must_use]
    pub fn pages(&self) -> &[CreditPageChange] {
        &self.pages
    }
    /// Exact queue effects, preserving each append/update occurrence.
    #[must_use]
    pub fn cleanup(&self) -> &[CreditCleanupChange] {
        &self.cleanup
    }
}

/// Continues exactly one required observation without resetting its account.
/// # Errors
/// Refuses a wrong/missing/corrupt observation, profile or allocation failure.
pub fn resume_identified_credit<'a>(
    prepared: PreparedIdentifiedCredit<'a>,
    observation: CreditProtocolObservation<'a>,
) -> Result<PreparedIdentifiedCredit<'a>, E> {
    super::decision::resume(prepared, observation)
}
