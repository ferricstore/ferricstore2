//! Fresh-only prepared policy install composition after retained request inspection.

use super::dispatch::after_gate;
use super::gate::{CommittedHeader, GateView, inspect_request};
use super::{DispatchPlan, PolicyRejection};
use crate::policy::installation::{
    InstallCommand, PolicyInstallPlan, PolicyLimits, PolicyValue, decide_install,
    validate_stored_policy,
};
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;

/// Coherent retained/control and stored policy generation authority.
#[derive(Clone, Copy, Debug)]
pub struct PolicyRequestView<'a> {
    /// Original retained/control namespace authority.
    pub gate: GateView<'a>,
    /// Coherent current installed value, if present.
    pub existing: Option<&'a PolicyValue>,
    /// Persisted allocator/migration generation high-water in this same group.
    pub migration_high_water: u64,
}
/// Verified stamped request, typed installation intent and actual prepared value.
#[derive(Clone, Copy, Debug)]
pub struct PolicyEnvelope<'a> {
    /// Required trusted stamped original request header.
    pub header: CommittedHeader<'a>,
    /// Versioned operation schema's normalized policy command.
    pub command: &'a InstallCommand,
    /// Actual immutable private bytes bound to their typed candidate by the decoder.
    pub prepared: &'a PolicyValue,
}
/// Scoped replay or fresh borrowed install/business-rejection decision.
pub type PolicyRequestPlan<'a> = DispatchPlan<'a, Result<PolicyInstallPlan<'a>, PolicyRejection>>;

/// Dispatches prepared policy installation only for a new scoped request.
///
/// # Errors
/// Corrupt authority/preparation/identity failure produces no control plan.
pub fn decide_policy_request<'a>(
    view: PolicyRequestView<'a>,
    envelope: PolicyEnvelope<'a>,
    policy_limits: PolicyLimits,
    limits: AdmissionLimits,
) -> Result<PolicyRequestPlan<'a>, ApplyError> {
    let gate = inspect_request(view.gate, envelope.header, limits)?;
    after_gate(gate, |_now| {
        if let Some(existing) = view.existing {
            validate_stored_policy(existing, &envelope.command.workflow_type, policy_limits)
                .map_err(|_| ApplyError::InvalidAuthority)?;
        }
        match decide_install(
            view.existing,
            view.migration_high_water,
            envelope.command,
            envelope.prepared,
            policy_limits,
        ) {
            Ok(plan) => Ok(Ok(plan)),
            Err(error) => match PolicyRejection::new(error) {
                Ok(rejection) => Ok(Err(rejection)),
                Err(_) => Err(ApplyError::InvalidAuthority),
            },
        }
    })
}
