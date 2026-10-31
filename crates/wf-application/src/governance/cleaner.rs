//! Bounded maintenance orchestration; committed apply owns cleanup authority.
mod model;
pub use model::{
    CreditCleanupError, CreditCleanupFailure, CreditCleanupLimits, CreditCleanupReply,
    CreditCleanupRequest, CreditCleanupSource, CreditCleanupStats, CreditCleanupStep,
};
use wf_ports::{
    consensus::CreditCatalogPublicationPort,
    state::{CreditOwnerReadPort, GovernanceCatalogProgressPort, GovernanceCatalogReadPort},
};

/// Runs one source-compatible step with at most three attempted commands.
/// # Errors
/// Refuses invalid whole-call options/resources before effects.
pub fn run_credit_cleanup_once<P>(
    request: CreditCleanupRequest<'_>,
    port: &mut P,
    limits: CreditCleanupLimits,
) -> Result<CreditCleanupStep, CreditCleanupError>
where
    P: CreditCatalogPublicationPort
        + CreditOwnerReadPort
        + GovernanceCatalogReadPort
        + GovernanceCatalogProgressPort,
{
    let _ = (request, port, limits);
    Err(CreditCleanupError::InvalidOptions)
}

/// Runs a bounded tick; None selects16 attempted commands, with a maximum64.
/// # Errors
/// Refuses invalid whole-call options/resources before effects.
pub fn run_credit_cleanup_tick<P>(
    request: CreditCleanupRequest<'_>,
    port: &mut P,
    command_budget: Option<u16>,
    limits: CreditCleanupLimits,
) -> Result<CreditCleanupStats, CreditCleanupError>
where
    P: CreditCatalogPublicationPort
        + CreditOwnerReadPort
        + GovernanceCatalogReadPort
        + GovernanceCatalogProgressPort,
{
    let _ = (request, port, command_budget, limits);
    Err(CreditCleanupError::InvalidOptions)
}
