//! Bounded maintenance orchestration; committed apply owns cleanup authority.
mod dispatch;
mod model;
mod plan;
mod step;
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
    plan::preflight(request, Some(3), limits)?;
    Ok(step::run(port, request, limits, 3, None, 0, 1))
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
    let budget = plan::preflight(request, command_budget, limits)?;
    let mut stats = CreditCleanupStats::default();
    let mut streak = None;
    let mut streak_pages = 0;
    let mut cycle_deleted = 0_u32;
    for _ in 0..(4 * budget + 4) {
        if stats.commands >= budget {
            break;
        }
        let mut step = step::run(
            port,
            request,
            limits,
            budget - stats.commands,
            streak,
            streak_pages,
            2,
        );
        stats.commands += step.commands;
        stats.deleted += u32::from(step.deleted);
        stats.errors += step.errors;
        stats.wrapped |= step.wrapped;
        cycle_deleted += u32::from(step.deleted);
        streak = step.streak.take();
        streak_pages = step.streak_pages;
        if step.wrapped && cycle_deleted == 0 {
            stats.caught_up = true;
            break;
        }
        if step.wrapped {
            cycle_deleted = 0;
        }
    }
    Ok(stats)
}
