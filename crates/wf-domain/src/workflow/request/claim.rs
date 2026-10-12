//! Header-first request dispatch for one bounded authoritative physical claim phase.

use crate::claim::apply::{
    ClaimApplyView, ClaimDecision, ClaimEnvelope, ClaimWorkLimits, decision,
};
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;

use super::DispatchPlan;
use super::dispatch::after_gate;
use super::gate::inspect_request;

/// Inspects retained/log replay before any current query, token or index guard.
///
/// Fresh empty and business-rejected phases finalize this request ID. Atomic
/// record/history/index/outcome/control persistence is an outer adapter duty.
/// The fourth argument is trusted replicated configuration, never entry authority.
///
/// # Errors
/// Operational authority, committed identity or host failure exposes no control plan.
pub fn decide_claim_request<'a>(
    view: ClaimApplyView<'a>,
    envelope: &ClaimEnvelope<'a>,
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
) -> Result<DispatchPlan<'a, ClaimDecision<'a>>, ApplyError> {
    let gate = inspect_request(view.gate, envelope.header, limits)?;
    after_gate(gate, |now| {
        decision::decide(view, envelope, now, limits, work)
    })
}
