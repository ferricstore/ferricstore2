//! Explicit source observations; no routing or paid/current-state certificate.

use super::{CreditCatalogEntry, CreditCatalogPage};
use crate::state::StateTransactionError;
use wf_domain::{governance::credit::CreditLimits, workflow::request::RequestScope};

/// Positive cumulative bounds for one coherent source guard.
#[derive(Clone, Copy, Debug)]
pub struct CreditCatalogReadLimits {
    /// Original owner decoder/work profile.
    pub credit: CreditLimits,
    /// Cumulative open/probe/advance work.
    pub maximum_steps: usize,
    /// Cumulative owned observation bytes, including discarded decoded owners.
    pub maximum_bytes: usize,
    /// Cumulative key/adapter temporary bytes.
    pub maximum_temporary_bytes: usize,
}
impl Default for CreditCatalogReadLimits {
    fn default() -> Self {
        Self {
            credit: CreditLimits::default(),
            maximum_steps: 10000,
            maximum_bytes: 64 * 1_048_576,
            maximum_temporary_bytes: 32 * 1_048_576,
        }
    }
}

/// Point classification in the exact original batch order, without owner clones.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogOwnerState {
    /// The exact original owner exists and its body/namespace were validated.
    Present,
    /// A well-formed intent has no committed owner; discard it when ACK succeeds.
    Missing,
    /// Localized invalid owner diagnosis; stop before this entry.
    /// A stricter adapter may instead fence/refuse the whole batch.
    Corrupt,
}

/// Server-owned source capability; every call verifies actual configured scope.
pub trait CreditCatalogReadPort {
    /// Returns one complete coherent source queue page under the given bounds.
    /// # Errors
    /// Refuses routing/capability, corruption, allocation or admission failures.
    fn credit_catalog_page(
        &mut self,
        scope: &RequestScope,
        limit: u16,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, StateTransactionError>;
    /// Classifies the exact original batch in one coherent guard and exact order.
    /// Decoded owners must be dropped individually; do not clone an owner map.
    /// # Errors
    /// Refuses malformed batches, foreign scope or source/read/work failures.
    fn credit_catalog_owners(
        &mut self,
        scope: &RequestScope,
        entries: &[CreditCatalogEntry],
        limits: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, StateTransactionError>;
}
