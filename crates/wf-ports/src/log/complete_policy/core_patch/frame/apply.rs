//! Ordered raw core-Patch composition; persistence remains an outer obligation.

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchFresh, PolicySchemaCharge, PolicySchemaLimits,
    prepare_complete_core_policy_patch_fresh,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlPlan, ControlState};
use wf_domain::workflow::request::DispatchDecision;
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, inspect_complete_core_policy_patch_gate,
};
use wf_types::version::LogIdentity;

use super::decode::consume_verified_frame;
use super::verify_frame;

/// Complete mathematical parent with original reply and coherent compare inputs.
///
/// No public constructor, transaction or durable publication is supplied.
pub struct CompleteCorePolicyPatchRequestPlan<'view> {
    expected_revision: u64,
    base_control: ControlState,
    control: ControlPlan,
    decision: DispatchDecision<'view, CompleteCorePolicyPatchFresh>,
}

impl<'view> CompleteCorePolicyPatchRequestPlan<'view> {
    /// Reads the exact coherent snapshot revision required by later atomic commit.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Reads original control authority required by the later atomic comparison.
    #[must_use]
    pub const fn base_control(&self) -> ControlState {
        self.base_control
    }

    /// Reads proposed control; exact log replay proposes no publication.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }

    /// Borrows Fresh owners or the exact original view-borrowed replay outcome.
    #[must_use]
    pub const fn decision(&self) -> &DispatchDecision<'view, CompleteCorePolicyPatchFresh> {
        &self.decision
    }

    /// Moves all proposed owners and compare inputs without copying reply bytes.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        u64,
        ControlState,
        ControlPlan,
        DispatchDecision<'view, CompleteCorePolicyPatchFresh>,
    ) {
        (
            self.expected_revision,
            self.base_control,
            self.control,
            self.decision,
        )
    }
}

/// Verifies raw bytes before exactly one paid retained-request inspection.
///
/// Only Fresh consumes the verified frame, reconstructs its original partial
/// command and inspects current/context/migration/window authority. Other decisions
/// preserve the original outcome and skip every Fresh phase. All calls thread the
/// received continuous account; a failure returns no proposed parent.
///
/// # Errors
/// Preserves private verification, gate, current/read/work/encoding/outcome failures.
/// No operational failure becomes a retained business response.
pub fn decide_complete_core_policy_patch_frame<'view>(
    view: CompleteCorePolicyPatchApplyView<'view>,
    encoded: &[u8],
    log: LogIdentity,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchRequestPlan<'view>, CompleteCorePolicyPatchApplyError> {
    use CompleteCorePolicyPatchApplyError::{Apply, Request};

    let verified = verify_frame(encoded, limits, admission, charge).map_err(Request)?;
    let (gate, intent) = {
        let header = verified.header(log);
        let intent = header.intent;
        let gate =
            inspect_complete_core_policy_patch_gate(view.gate, header, limits, admission, charge)
                .map_err(|error| match error {
                ChargedRequestGateError::Apply(error) => Apply(error),
                ChargedRequestGateError::Schema(error) => Request(error.into()),
            })?;
        (gate, intent)
    };
    let expected_revision = view.revision;
    let base_control = view.gate.control;
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            let input = consume_verified_frame(verified, limits, admission, charge)
                .map_err(Request)?
                .into_input();
            DispatchDecision::Fresh(prepare_complete_core_policy_patch_fresh(
                view,
                input,
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
    Ok(CompleteCorePolicyPatchRequestPlan {
        expected_revision,
        base_control,
        control,
        decision,
    })
}
