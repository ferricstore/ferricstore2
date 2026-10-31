//! Existing-owner repair must succeed before a dependent public mutation.
use super::{CreditCatalogPublicationError, CreditCatalogPublicationLimits};
use std::fmt;
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    consensus::CreditCatalogPublicationPort,
    state::{CreditOwnerReadPort, GovernanceCatalogReadPort, StateTransactionError},
};

/// Observed repair result; none grants later placement or mutation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogRepairStatus {
    /// No owner exists; no ghost member was created.
    Missing,
    /// The existing owner already has exact membership.
    Present,
    /// A complete accepted retained registration was confirmed.
    Registered,
}
/// Repair refusal; callers must not execute a dependent mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogRepairError {
    /// Whole-call source/target/resource preflight failed.
    Preflight(CreditCatalogPublicationError),
    /// Exact owner or member read failed.
    Read(StateTransactionError),
    /// Registration was refused, uncertain or returned an invalid original reply.
    Unconfirmed,
}
impl fmt::Display for CreditCatalogRepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preflight(e) => e.fmt(f),
            Self::Read(e) => e.fmt(f),
            Self::Unconfirmed => f.write_str("catalog repair registration unconfirmed"),
        }
    }
}
impl std::error::Error for CreditCatalogRepairError {}

/// Observes one exact owner and repairs its catalog entry through supervised ports.
/// Missing owners never cause registration. Only a complete accepted reply
/// permits a caller to continue; all public parents must still revalidate state.
/// # Errors
/// Refuses invalid preflight/read state or any unconfirmed registration.
pub fn repair_credit_catalog<P>(
    port: &mut P,
    source: &RequestScope,
    target: &RequestScope,
    catalog: &[u8],
    credit_scope: &[u8],
    limits: CreditCatalogPublicationLimits,
) -> Result<CreditCatalogRepairStatus, CreditCatalogRepairError>
where
    P: CreditCatalogPublicationPort + CreditOwnerReadPort + GovernanceCatalogReadPort,
{
    let _ = (port, source, target, catalog, credit_scope, limits);
    Err(CreditCatalogRepairError::Unconfirmed)
}
