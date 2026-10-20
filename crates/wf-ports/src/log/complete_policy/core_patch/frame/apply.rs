//! Ordered raw core-Patch composition; persistence remains an outer obligation.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchFresh, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    StoredPolicyValue, prepare_complete_core_policy_patch_fresh_with_query,
};
use wf_domain::policy::installation::MAX_ENCODED_POLICY_BYTES;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlPlan, ControlState};
use wf_domain::workflow::request::DispatchDecision;
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, inspect_complete_core_policy_patch_gate,
};
use wf_types::version::LogIdentity;

use super::decode::consume_verified_frame;
use super::verify_frame;

mod point;

pub use point::{
    CompleteCorePolicyPatchPointInspection, FreshCompleteCorePolicyPatchFrame,
    decide_complete_core_policy_patch_point_fresh, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};

/// Complete mathematical parent with original reply and coherent compare inputs.
///
/// No public constructor, transaction or durable publication is supplied.
pub struct CompleteCorePolicyPatchRequestPlan<'view> {
    expected_revision: u64,
    base_control: ControlState,
    control: ControlPlan,
    decision: DispatchDecision<'view, CompleteCorePolicyPatchFresh>,
    retained_query: Option<Vec<u8>>,
    predecessor_identity: Option<(usize, [u8; 32])>,
    used_history_max_events_limit: Option<u32>,
    used_migrations: Option<(u64, u64)>,
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

    /// Reads history context only when Fresh preparation reached its valid getter.
    /// The consuming store still checks this mathematical value against its own.
    #[must_use]
    pub const fn used_history_max_events_limit(&self) -> Option<u32> {
        self.used_history_max_events_limit
    }

    /// Reads migration context only after both original getters succeeded.
    #[must_use]
    pub const fn used_migration_generations(&self) -> Option<(u64, u64)> {
        self.used_migrations
    }

    /// Repeats Fresh response correspondence using this plan's sealed query.
    /// # Errors
    /// Preserves the same grammar, profile and owner refusals as the matcher.
    pub fn response_matches(
        &self,
        profiles: (PolicySchemaLimits, AdmissionLimits),
        charge: &mut PolicySchemaCharge,
        maximum_policy_owner_bytes: usize,
    ) -> Result<bool, crate::log::CodecError> {
        let DispatchDecision::Fresh(fresh) = &self.decision else {
            return Ok(false);
        };
        super::super::response::response_matches_for_query(
            fresh,
            profiles,
            charge,
            maximum_policy_owner_bytes,
            self.workflow_type(),
        )
    }

    /// Borrows the original Fresh query; non-Fresh has no current-policy query.
    /// Rejected queries retain their original owner, including an empty type.
    #[must_use]
    pub fn workflow_type(&self) -> Option<&[u8]> {
        match &self.decision {
            DispatchDecision::Fresh((Some((_, StoredPolicyValue::Complete(value))), _)) => {
                Some(&value.data().workflow_type)
            }
            DispatchDecision::Fresh(_) => self.retained_query.as_deref(),
            _ => None,
        }
    }

    /// Checks correspondence to the predecessor used during pure preparation.
    ///
    /// This fixed digest uses the same collision assumption as private frames.
    /// Each actual hash is bounded native O(P) work, outside the logical g tariff;
    /// equality does not prove stored origin or committed authority.
    #[must_use]
    pub fn matches_predecessor(&self, predecessor: Option<&StoredPolicyValue>) -> bool {
        match (self.predecessor_identity, predecessor) {
            (None, None) => true,
            (Some((length, digest)), Some(value))
                if length <= MAX_ENCODED_POLICY_BYTES && value.encoded().len() == length =>
            {
                digest == <[u8; 32]>::from(Sha256::digest(value.encoded()))
            }
            _ => false,
        }
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
    let mut retained_query = None;
    let mut predecessor_identity = None;
    let mut used_history_max_events_limit = None;
    let mut used_migrations = None;
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            let input = consume_verified_frame(verified, limits, admission, charge)
                .map_err(Request)?
                .into_input();
            let (prepared, query, current, used_history, migrations) =
                prepare_complete_core_policy_patch_fresh_with_query(
                    view,
                    input,
                    (intent, log, control.execution_time()),
                    limits,
                    admission,
                    charge,
                )?;
            retained_query = query;
            predecessor_identity = capture_predecessor(current)?;
            used_history_max_events_limit = used_history;
            used_migrations = migrations;
            DispatchDecision::Fresh(prepared)
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
        retained_query,
        predecessor_identity,
        used_history_max_events_limit,
        used_migrations,
    })
}

fn capture_predecessor(
    current: Option<&StoredPolicyValue>,
) -> Result<Option<(usize, [u8; 32])>, CompleteCorePolicyPatchApplyError> {
    current
        .map(|value| {
            let length = value.encoded().len();
            if length > MAX_ENCODED_POLICY_BYTES {
                return Err(CompleteCorePolicyPatchApplyError::Request(
                    PolicySchemaError::InvalidAuthority.into(),
                ));
            }
            // The validated current owner bounds this additional native hash walk.
            Ok((length, Sha256::digest(value.encoded()).into()))
        })
        .transpose()
}
