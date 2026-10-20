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

mod point;
pub use point::{
    FreshOriginalPolicySetFrame, OriginalPolicySetPointInspection,
    decide_original_policy_set_frame_bounded, decide_original_policy_set_point_fresh_bounded,
    inspect_original_policy_set_point_frame_bounded,
    prepare_original_policy_set_point_frame_bounded,
};

/// Complete proposed owners and comparison inputs, borrowing only gate replay.
///
/// No constructor or physical publication is supplied. The frame, current value,
/// decoder tables and normalization layout can all drop before this plan escapes.
pub struct OriginalPolicySetRequestPlan<'view> {
    expected_revision: u64,
    base_control: ControlState,
    control: ControlPlan,
    decision: DispatchDecision<'view, OriginalPolicySetFresh>,
    workflow_type: Option<Vec<u8>>,
    predecessor: Option<(usize, [u8; 32])>,
    used_history: Option<u32>,
    used_migrations: Option<(u64, u64)>,
    portable_profile: Option<u32>,
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

    /// Borrows the original Fresh query, including a rejected query.
    #[must_use]
    pub fn workflow_type(&self) -> Option<&[u8]> {
        self.workflow_type.as_deref()
    }

    /// Compares exact predecessor presence and its bounded raw digest.
    #[must_use]
    pub fn matches_predecessor(&self, current: Option<&[u8]>) -> bool {
        use sha2::{Digest, Sha256};
        if self.portable_profile.is_none() {
            return false;
        }
        match (self.predecessor, current) {
            (None, None) => true,
            (Some((length, digest)), Some(raw)) if raw.len() <= 263_168 => {
                length == raw.len() && digest == <[u8; 32]>::from(Sha256::digest(raw))
            }
            _ => false,
        }
    }

    /// Reads only successfully reached, range-valid history context.
    #[must_use]
    pub const fn used_history_max_events_limit(&self) -> Option<u32> {
        self.used_history
    }

    /// Reads only successfully reached active/completed migration context.
    #[must_use]
    pub const fn used_migration_generations(&self) -> Option<(u64, u64)> {
        self.used_migrations
    }

    /// Reads the fixed portable profile of a new sealed Fresh preparation.
    #[must_use]
    pub const fn portable_profile_id(&self) -> Option<u32> {
        self.portable_profile
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
        workflow_type: None,
        predecessor: None,
        used_history: None,
        used_migrations: None,
        portable_profile: None,
    })
}
