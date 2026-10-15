//! One verified frame and request gate before original whole-flow Fresh work.

use wf_domain::policy::complete::{
    CompletePolicyReplaceApplyError, OriginalCompletePolicyReplaceApplyError,
    OriginalPolicySetApplyView, OriginalPolicySetFresh, PolicySchemaCharge, PolicySchemaLimits,
    prepare_original_policy_set_fresh,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlPlan, ControlState};
use wf_domain::workflow::request::DispatchDecision;
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, inspect_complete_core_policy_patch_gate,
};
use wf_types::version::LogIdentity;

use super::{VerifiedOriginalPolicySetFrame, verify_original_policy_set_frame};

/// Complete proposed owners and comparison inputs, borrowing only gate replay.
///
/// No constructor or physical publication is supplied. The frame, current value,
/// decoder tables and normalization layout can all drop before this plan escapes.
pub struct OriginalPolicySetRequestPlan<'view> {
    expected_revision: u64,
    base_control: ControlState,
    control: ControlPlan,
    decision: DispatchDecision<'view, OriginalPolicySetFresh>,
}

impl<'view> OriginalPolicySetRequestPlan<'view> {
    /// Reads the coherent revision required by a later atomic comparison.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Reads original committed control required by a later atomic comparison.
    #[must_use]
    pub const fn base_control(&self) -> ControlState {
        self.base_control
    }

    /// Reads proposed control; exact log replay proposes no publication.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }

    /// Borrows complete Fresh owners or the original retained replay decision.
    #[must_use]
    pub const fn decision(&self) -> &DispatchDecision<'view, OriginalPolicySetFresh> {
        &self.decision
    }

    /// Moves all proposed owners and comparison inputs without copying them.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        u64,
        ControlState,
        ControlPlan,
        DispatchDecision<'view, OriginalPolicySetFresh>,
    ) {
        (
            self.expected_revision,
            self.base_control,
            self.control,
            self.decision,
        )
    }
}

/// Verifies once and inspects the retained-request gate once on one account.
///
/// Non-Fresh returns before all current/context/migration/window work. Fresh
/// consumes the verified original header and returns independently owned original
/// ETF installation/response bytes; physical atomic publication remains external.
///
/// # Errors
/// Preserves verifier, gate and Fresh operational errors without a partial plan
/// or conversion of operational failures into retained business diagnoses.
pub fn decide_original_policy_set_frame<'view>(
    view: OriginalPolicySetApplyView<'view, '_, '_, '_>,
    encoded: &[u8],
    log: LogIdentity,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetRequestPlan<'view>, OriginalCompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Apply, Request};
    use OriginalCompletePolicyReplaceApplyError::Replace;

    let base = view.base();
    let verified =
        verify_original_policy_set_frame(encoded, view.orders(), limits, admission, charge)
            .map_err(|error| Replace(Request(error)))?;
    let (gate, intent) = {
        let header = verified.header(log);
        let intent = header.intent;
        let gate =
            inspect_complete_core_policy_patch_gate(base.gate, header, limits, admission, charge)
                .map_err(|error| match error {
                ChargedRequestGateError::Apply(error) => Replace(Apply(error)),
                ChargedRequestGateError::Schema(error) => Replace(Request(error.into())),
            })?;
        (gate, intent)
    };
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            let VerifiedOriginalPolicySetFrame {
                header, isolation, ..
            } = verified;
            drop(isolation);
            DispatchDecision::Fresh(prepare_original_policy_set_fresh(
                view,
                header,
                (intent, log, control.execution_time()),
                limits,
                admission,
                charge,
            )?)
        }
        GateDecision::Replay(original) => DispatchDecision::Replay(original),
        GateDecision::IntentConflict => DispatchDecision::IntentConflict,
        GateDecision::Expired => DispatchDecision::Expired,
        GateDecision::LogReplay => DispatchDecision::LogReplay,
    };
    Ok(OriginalPolicySetRequestPlan {
        expected_revision: base.revision,
        base_control: base.gate.control,
        control,
        decision,
    })
}
