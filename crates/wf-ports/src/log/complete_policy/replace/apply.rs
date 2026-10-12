//! Closed raw Replace parent; physical comparison and publication remain external.

use wf_domain::policy::complete::{
    CompletePolicyReplaceApplyError, CompletePolicyReplaceApplyView, CompletePolicyReplaceFresh,
    PolicySchemaCharge, PolicySchemaLimits, prepare_complete_policy_replace_fresh,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlPlan, ControlState};
use wf_domain::workflow::request::DispatchDecision;
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, inspect_complete_core_policy_patch_gate,
};
use wf_types::version::LogIdentity;

use super::decode::consume_verified_frame;
use super::verify_complete_policy_replace_frame;

/// Complete proposed mathematical bundle with coherent revision/control comparisons.
///
/// Replay borrows only external retained authority. No constructor, physical
/// reader, transaction or durable publication is provided.
pub struct CompletePolicyReplaceRequestPlan<'view> {
    expected_revision: u64,
    base_control: ControlState,
    control: ControlPlan,
    decision: DispatchDecision<'view, CompletePolicyReplaceFresh>,
}

impl<'view> CompletePolicyReplaceRequestPlan<'view> {
    /// Reads the original coherent revision required by a later atomic commit.
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

    /// Borrows the complete Fresh owners or immutable original replay decision.
    #[must_use]
    pub const fn decision(&self) -> &DispatchDecision<'view, CompletePolicyReplaceFresh> {
        &self.decision
    }

    /// Moves the complete proposed bundle and its compare inputs without copying.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        u64,
        ControlState,
        ControlPlan,
        DispatchDecision<'view, CompletePolicyReplaceFresh>,
    ) {
        (
            self.expected_revision,
            self.base_control,
            self.control,
            self.decision,
        )
    }
}

/// Verifies once and inspects the retained-request gate once on the same account.
///
/// Non-Fresh returns before reconstruction or current/context/migration/window
/// reads. Only Fresh consumes this verified frame and constructs original
/// replacement/outcome owners. No result establishes physical installation.
///
/// # Errors
/// Preserves verification, gate and Fresh operational refusals without a partial
/// parent or conversion of operational failures into retained business status.
pub fn decide_complete_policy_replace_frame<'view>(
    view: CompletePolicyReplaceApplyView<'view>,
    encoded: &[u8],
    log: LogIdentity,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceRequestPlan<'view>, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Apply, Request};

    let verified = verify_complete_policy_replace_frame(encoded, limits, admission, charge)
        .map_err(Request)?;
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
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            let input = consume_verified_frame(verified, limits, admission, charge)
                .map_err(Request)?
                .into_input();
            DispatchDecision::Fresh(prepare_complete_policy_replace_fresh(
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
    Ok(CompletePolicyReplaceRequestPlan {
        expected_revision: view.revision,
        base_control: view.gate.control,
        control,
        decision,
    })
}
