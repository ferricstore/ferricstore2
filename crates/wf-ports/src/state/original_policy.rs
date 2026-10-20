//! Original full-backing changes checked again by a consuming actual guard.

use super::{GroupApplyTransaction, StateTransactionError as Error};
use crate::log::{
    CodecError, OriginalPolicySetRequestPlan, original_policy_set_response_matches,
    original_policy_set_response_status,
};
use wf_domain::policy::complete::{
    OriginalFlowPolicyInstall, PolicySchemaCharge, PolicySchemaLimits,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, plan_control};
use wf_domain::workflow::request::{DispatchDecision, RequestScope, RetainedOutcome};
use wf_types::identity::TenantId;

/// Actual immutable profile/raw reads and one consumed atomic publication.
pub trait GroupOriginalPolicyTransaction: GroupApplyTransaction {
    /// Reads actual original-policy layout/profile capability.
    /// # Errors
    /// Preserves unavailable, unsupported or corrupt actual authority.
    fn original_policy_profile(&self) -> Result<u32, Error>;
    /// Reads the complete actual raw predecessor, preserving format and presence.
    /// # Errors
    /// Charges temporary validation and returned owners before allocation.
    fn original_policy_current(
        &self,
        tenant: &TenantId,
        workflow_type: &[u8],
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<Vec<u8>>, Error>;
    /// Repeats correspondence using actual control/config/raw and publishes once.
    /// # Errors
    /// Returns proven layout only after durable commit; uncertainty fences reuse.
    fn commit_original_policy(
        self,
        changes: GroupOriginalPolicyChanges<'_>,
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<u16, Error>;
}

/// Supplied mathematical changes; construction grants no physical authority.
pub struct GroupOriginalPolicyChanges<'view> {
    scope: &'view RequestScope,
    expected: ControlState,
    predecessor: Option<&'view [u8]>,
    plan: OriginalPolicySetRequestPlan<'view>,
}

impl<'view> GroupOriginalPolicyChanges<'view> {
    /// Checks exact original query/raw/control and complete original response.
    /// # Errors
    /// Distinguishes narrower profiles from incoherent supplied changes.
    pub fn new(
        scope: &'view RequestScope,
        expected: ControlState,
        predecessor: Option<&'view [u8]>,
        plan: OriginalPolicySetRequestPlan<'view>,
        profiles: (AdmissionLimits, PolicySchemaLimits),
        charge: &mut PolicySchemaCharge,
        maximum_policy_owner_bytes: usize,
    ) -> Result<Self, Error> {
        if charge.profile() != profiles.1
            || plan.base_control() != expected
            || plan.expected_revision() != expected.position().get()
        {
            return Err(Error::InvalidChanges);
        }
        let control = plan.control();
        let next = control.next();
        let identity = next.last_entry().ok_or(Error::InvalidChanges)?;
        let stamp = next.last_entry_stamp().ok_or(Error::InvalidChanges)?;
        if plan_control(expected, identity, stamp).map_err(|_| Error::InvalidChanges)? != control
            || control.is_log_replay() != matches!(plan.decision(), DispatchDecision::LogReplay)
        {
            return Err(Error::InvalidChanges);
        }
        match plan.decision() {
            DispatchDecision::Fresh((install, outcome)) => {
                validate_outcome(scope, outcome, profiles.0)?;
                validate_late_owners(install.as_ref(), outcome, profiles.0)?;
                if plan.portable_profile_id() != Some(1)
                    || plan.workflow_type().is_none()
                    || !plan.matches_predecessor(predecessor)
                    || outcome.original_position() != identity.log()
                    || outcome.original_time() != control.execution_time()
                    || !original_policy_set_response_matches(
                        &plan,
                        profiles.1,
                        charge,
                        maximum_policy_owner_bytes,
                    )
                    .map_err(codec_error)?
                {
                    return Err(Error::InvalidChanges);
                }
            }
            decision => {
                if predecessor.is_some() {
                    return Err(Error::InvalidChanges);
                }
                if let DispatchDecision::Replay(outcome) = decision {
                    validate_outcome(scope, outcome, profiles.0)?;
                    let status = original_policy_set_response_status(
                        outcome.response(),
                        profiles.1,
                        charge,
                        maximum_policy_owner_bytes,
                    )
                    .map_err(codec_error)?;
                    if status != outcome.status() {
                        return Err(Error::InvalidChanges);
                    }
                }
            }
        }
        Ok(Self {
            scope,
            expected,
            predecessor,
            plan,
        })
    }

    /// Reads exact supplied tenant/group/recovery identity.
    #[must_use]
    pub const fn scope(&self) -> &'view RequestScope {
        self.scope
    }
    /// Reads exact supplied processing predecessor.
    #[must_use]
    pub const fn expected(&self) -> ControlState {
        self.expected
    }
    /// Borrows the original full raw predecessor, preserving absence.
    #[must_use]
    pub const fn predecessor(&self) -> Option<&'view [u8]> {
        self.predecessor
    }
    /// Borrows the original immutable plan.
    #[must_use]
    pub const fn plan(&self) -> &OriginalPolicySetRequestPlan<'view> {
        &self.plan
    }
    /// Moves the original plan without a clone or commit certificate.
    #[must_use]
    pub fn into_plan(self) -> OriginalPolicySetRequestPlan<'view> {
        self.plan
    }
    /// Borrows only an accepted Fresh full original installation.
    #[must_use]
    pub fn business(&self) -> Option<&OriginalFlowPolicyInstall> {
        match self.plan.decision() {
            DispatchDecision::Fresh((install, _)) => install.as_ref(),
            _ => None,
        }
    }
    /// Borrows only an original Fresh response owner to publish.
    #[must_use]
    pub fn outcome(&self) -> Option<&RetainedOutcome> {
        match self.plan.decision() {
            DispatchDecision::Fresh((_, outcome)) => Some(outcome),
            _ => None,
        }
    }
    /// Proposes no publication for exact last-log replay.
    #[must_use]
    pub fn control(&self) -> Option<ControlState> {
        (!self.plan.control().is_log_replay()).then_some(self.plan.control().next())
    }
}

fn validate_outcome(
    scope: &RequestScope,
    outcome: &RetainedOutcome,
    admission: AdmissionLimits,
) -> Result<(), Error> {
    if outcome.scope() != scope
        || outcome.intent().version() != 4
        || outcome.response_version() != 1
    {
        return Err(Error::InvalidChanges);
    }
    if outcome.response().len() > admission.decision_bytes() {
        return Err(Error::Limit);
    }
    Ok(())
}
fn codec_error(error: CodecError) -> Error {
    match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => Error::Limit,
        CodecError::Allocation => Error::Allocation,
        CodecError::UnsupportedPolicyProfile => Error::UnsupportedPolicyProfile,
        _ => Error::InvalidChanges,
    }
}

fn validate_late_owners(
    install: Option<&OriginalFlowPolicyInstall>,
    outcome: &RetainedOutcome,
    admission: AdmissionLimits,
) -> Result<(), Error> {
    let mut total = outcome.response().len();
    for bytes in [
        outcome.scope().tenant().as_bytes(),
        outcome.scope().group().as_bytes(),
        outcome.scope().recovery().as_bytes(),
        outcome.request().as_bytes(),
    ] {
        total = total.checked_add(bytes.len()).ok_or(Error::Limit)?;
    }
    if let Some(install) = install {
        total = total
            .checked_add(install.policy_key().len())
            .and_then(|n| n.checked_add(install.encoded().len()))
            .ok_or(Error::Limit)?;
    }
    if total > admission.decision_bytes() {
        return Err(Error::Limit);
    }
    Ok(())
}
