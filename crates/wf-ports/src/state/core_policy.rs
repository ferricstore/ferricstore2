//! Same-guard policy publication and mathematical change correspondence.

use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, plan_control};
use wf_domain::workflow::request::{DispatchDecision, RequestScope, RetainedOutcome};
use wf_types::identity::{RequestId, TenantId};

use crate::log::{
    CodecError, CompleteCorePolicyPatchRequestPlan, complete_core_policy_patch_response_matches,
    complete_core_policy_patch_retained_status,
};

use super::{GroupApplyTransaction, StateTransactionError};

/// Captured policy context and consumed publication on one exclusive revision.
pub trait GroupCorePolicyTransaction: GroupApplyTransaction {
    /// Reads the immutable complete-policy ceiling captured by this actual guard.
    fn core_policy_schema_limits(&self) -> PolicySchemaLimits;
    /// Reads the actual immutable configured history bound.
    fn core_policy_history_max_events_limit(&self) -> u32;
    /// Reads supported layout mode from this strictly opened owner.
    /// # Errors
    /// Refuses unavailable or unsupported authority; it creates no migration fact.
    fn core_policy_layout(&self) -> Result<u16, StateTransactionError>;
    /// Reads one original outcome using the same external operation account.
    /// # Errors
    /// Preserves actual corruption, ownership and continuous-work refusals.
    fn core_policy_outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError>;
    /// Revalidates against this guard and publishes all selected changes atomically.
    ///
    /// Success returns the actually committed layout. The caller updates its
    /// cached mode only after this consuming operation proves publication.
    /// # Errors
    /// Distinguishes known abort, uncertain commit and operational authority errors.
    fn commit_core_policy(
        self,
        changes: &GroupCorePolicyChanges<'_, '_, '_>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<u16, StateTransactionError>;
}

/// Supplied mathematical changes; construction grants no durable authority.
pub struct GroupCorePolicyChanges<'scope, 'old, 'retained> {
    scope: &'scope RequestScope,
    expected: ControlState,
    predecessor: Option<&'old CompletePolicyValue>,
    plan: CompleteCorePolicyPatchRequestPlan<'retained>,
}

impl<'scope, 'old, 'retained> GroupCorePolicyChanges<'scope, 'old, 'retained> {
    /// Checks the original receipt, exact control and full predecessor relation.
    ///
    /// The caller admits temporary decoder ownership before this function. The
    /// consumed backend repeats checks using its actual captured profile.
    /// # Errors
    /// Narrow profiles return `Limit`; incoherent changes return `InvalidChanges`.
    pub fn new(
        scope: &'scope RequestScope,
        expected: ControlState,
        predecessor: Option<&'old CompletePolicyValue>,
        plan: CompleteCorePolicyPatchRequestPlan<'retained>,
        profiles: (PolicySchemaLimits, AdmissionLimits),
        charge: &mut PolicySchemaCharge,
        maximum_validation_owner_bytes: usize,
    ) -> Result<Self, StateTransactionError> {
        if charge.profile() != profiles.0
            || plan.base_control() != expected
            || plan.expected_revision() != expected.position().get()
        {
            return Err(StateTransactionError::InvalidChanges);
        }
        let control = plan.control();
        let next = control.next();
        let identity = next
            .last_entry()
            .ok_or(StateTransactionError::InvalidChanges)?;
        let stamp = next
            .last_entry_stamp()
            .ok_or(StateTransactionError::InvalidChanges)?;
        if plan_control(expected, identity, stamp)
            .map_err(|_| StateTransactionError::InvalidChanges)?
            != control
            || control.is_log_replay() != matches!(plan.decision(), DispatchDecision::LogReplay)
        {
            return Err(StateTransactionError::InvalidChanges);
        }
        match plan.decision() {
            DispatchDecision::Fresh(fresh) => {
                let (candidate, outcome) = fresh;
                let query = plan
                    .workflow_type()
                    .ok_or(StateTransactionError::InvalidChanges)?;
                if !plan.matches_predecessor(predecessor)
                    || predecessor.is_some_and(|old| old.data().workflow_type != query)
                {
                    return Err(StateTransactionError::InvalidChanges);
                }
                validate_scope(scope, outcome)?;
                if outcome.original_position() != identity.log()
                    || outcome.original_time() != control.execution_time()
                    || !complete_core_policy_patch_response_matches(
                        fresh,
                        profiles,
                        charge,
                        maximum_validation_owner_bytes,
                    )
                    .map_err(codec_error)?
                {
                    return Err(StateTransactionError::InvalidChanges);
                }
                if let Some((generation, value)) = candidate
                    && (*generation != predecessor.map_or(0, CompletePolicyValue::generation)
                        || predecessor.is_some_and(|old| {
                            old.data().workflow_type != value.data().workflow_type
                        }))
                {
                    return Err(StateTransactionError::InvalidChanges);
                }
            }
            decision => {
                if predecessor.is_some() {
                    return Err(StateTransactionError::InvalidChanges);
                }
                if let DispatchDecision::Replay(outcome) = decision {
                    validate_scope(scope, outcome)?;
                    let status = complete_core_policy_patch_retained_status(
                        outcome.intent(),
                        outcome.response_version(),
                        outcome.response(),
                        profiles,
                        charge,
                        maximum_validation_owner_bytes,
                    )
                    .map_err(codec_error)?;
                    if status != outcome.status() {
                        return Err(StateTransactionError::InvalidChanges);
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

    /// Reads the exact tenant/group/recovery namespace.
    #[must_use]
    pub const fn scope(&self) -> &'scope RequestScope {
        self.scope
    }
    /// Reads the exact processing predecessor.
    #[must_use]
    pub const fn expected(&self) -> ControlState {
        self.expected
    }
    /// Borrows the original full policy for actual backend correspondence.
    #[must_use]
    pub const fn predecessor(&self) -> Option<&'old CompletePolicyValue> {
        self.predecessor
    }
    /// Borrows all original classification and response owners.
    #[must_use]
    pub const fn plan(&self) -> &CompleteCorePolicyPatchRequestPlan<'retained> {
        &self.plan
    }
    /// Moves the mathematical plan without granting a commit certificate.
    #[must_use]
    pub fn into_plan(self) -> CompleteCorePolicyPatchRequestPlan<'retained> {
        self.plan
    }
    /// Borrows only a Fresh accepted policy and its expected generation.
    #[must_use]
    pub fn business(&self) -> Option<&(u64, CompletePolicyValue)> {
        match self.plan.decision() {
            DispatchDecision::Fresh((candidate, _)) => candidate.as_ref(),
            _ => None,
        }
    }
    /// Borrows the immutable outcome only for Fresh publication.
    #[must_use]
    pub fn outcome(&self) -> Option<&RetainedOutcome> {
        match self.plan.decision() {
            DispatchDecision::Fresh((_, outcome)) => Some(outcome),
            _ => None,
        }
    }
    /// Proposed processing control; exact last-log replay has no writes.
    #[must_use]
    pub fn control(&self) -> Option<ControlState> {
        (!self.plan.control().is_log_replay()).then_some(self.plan.control().next())
    }
}

fn validate_scope(
    scope: &RequestScope,
    outcome: &RetainedOutcome,
) -> Result<(), StateTransactionError> {
    if outcome.scope() != scope
        || outcome.intent().version() != 2
        || outcome.response_version() != 1
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}

fn codec_error(error: CodecError) -> StateTransactionError {
    match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => {
            StateTransactionError::Limit
        }
        CodecError::Allocation => StateTransactionError::Allocation,
        _ => StateTransactionError::InvalidChanges,
    }
}
