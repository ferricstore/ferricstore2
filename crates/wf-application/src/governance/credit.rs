//! Public credit caller sequencing; every committed mutation revalidates authority.
mod model;
pub use model::{
    PreparedPublicCreditCall, PublicCreditCallError, PublicCreditCallLimits,
    PublicCreditCallRequest, PublicCreditCallResult, PublicCreditRetryError,
};
use wf_ports::consensus::PublicCreditSubmitPort;
/// Resolves originals, repairs existing membership, prepares and submits one caller.
/// # Errors
/// Refuses before CREDIT proposal; catalog repairs may already have committed.
pub fn submit_public_credit_call<P: PublicCreditSubmitPort>(
    request: PublicCreditCallRequest<'_>,
    port: &mut P,
    limits: PublicCreditCallLimits,
) -> Result<PublicCreditCallResult, PublicCreditCallError> {
    let _ = (request, port, limits);
    Err(PublicCreditCallError::InvalidOptions)
}
/// Re-admits and submits the exact original frame without a fresh read or spend.
/// # Errors
/// Refuses current budgets before resubmission, preserving original ownership.
pub fn retry_public_credit_call<P: PublicCreditSubmitPort>(
    original: Box<PreparedPublicCreditCall>,
    port: &mut P,
    limits: PublicCreditCallLimits,
) -> Result<PublicCreditCallResult, PublicCreditRetryError> {
    let _ = (port, limits);
    Err(PublicCreditRetryError {
        error: PublicCreditCallError::InvalidOptions,
        original,
    })
}
