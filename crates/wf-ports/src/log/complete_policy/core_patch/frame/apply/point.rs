//! Bounded one-point classification followed by Fresh-only command ownership.

use super::super::{DecodedCompleteCorePolicyPatchFrame, VerifiedCompleteCorePolicyPatchFrame};
use super::{
    CompleteCorePolicyPatchRequestPlan, capture_predecessor, consume_verified_frame, verify_frame,
};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError as Error, CompleteCorePolicyPatchFreshView,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, StoredPolicyValue,
    complete_core_policy_patch_frame_decode_bound,
    prepare_complete_core_policy_patch_point_fresh_with_query,
};
use wf_domain::policy::installation::MAX_ENCODED_POLICY_BYTES;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlPlan, ControlState};
use wf_domain::workflow::request::DispatchDecision;
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, PointGateView,
    inspect_complete_core_policy_patch_point_gate,
};
use wf_types::version::LogIdentity;

/// Result of the actual point gate; only Fresh owns the reconstructed command.
pub enum CompleteCorePolicyPatchPointInspection<'input, 'retained> {
    /// Existing retained/control classification without current-policy reads.
    Complete(Box<CompleteCorePolicyPatchRequestPlan<'retained>>),
    /// Verified original command after one actual Fresh classification.
    Fresh(Box<FreshCompleteCorePolicyPatchFrame<'input>>),
}

/// Original command owners sealed to one actual Fresh point classification.
///
/// This mathematical result grants no storage or committed-origin authority.
pub struct FreshCompleteCorePolicyPatchFrame<'input> {
    decoded: DecodedCompleteCorePolicyPatchFrame<'input>,
    log: LogIdentity,
    base_control: ControlState,
    control: ControlPlan,
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    workspace_limit: usize,
    decoder_bound: usize,
}

impl<'input> FreshCompleteCorePolicyPatchFrame<'input> {
    /// Borrows the original decoded command and its verified bytes.
    #[must_use]
    pub const fn decoded(&self) -> &DecodedCompleteCorePolicyPatchFrame<'input> {
        &self.decoded
    }

    /// Reads the exact point-gate control proposal.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }

    /// Reads the actual coherent predecessor supplied to the point gate.
    #[must_use]
    pub const fn base_control(&self) -> ControlState {
        self.base_control
    }
}

/// Admits header ownership before the existing full borrowed verification.
///
/// `workspace` includes the selected schema scratch envelope; a physical caller
/// has already reserved its independent cumulative read/staging/adapter budgets.
/// # Errors
/// Preserves profile, workspace, grammar, hashing and continuous-work refusals.
pub fn prepare_complete_core_policy_patch_point_frame<'input>(
    encoded: &'input [u8],
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> Result<VerifiedCompleteCorePolicyPatchFrame<'input>, Error> {
    profile(schema, charge)?;
    let header = scaled(2048, encoded.len(), 3)?;
    admit(sum(&[scratch(schema)?, header])?, workspace)?;
    verify_frame(encoded, schema, admission, charge).map_err(Error::Request)
}

/// Runs exactly one paid point gate before any owning original-command decode.
///
/// Retained decisions drop the verified header and never materialize a Patch.
/// Fresh pays the real second grammar walk before its prospective decoder bound.
/// # Errors
/// Preserves actual gate authority, workspace, decoding and monotone work errors.
pub fn inspect_complete_core_policy_patch_point<'input, 'retained>(
    verified: VerifiedCompleteCorePolicyPatchFrame<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> Result<CompleteCorePolicyPatchPointInspection<'input, 'retained>, Error> {
    let (schema, admission) = profiles;
    profile(schema, charge)?;
    let base_control = view.control;
    let gate = inspect_complete_core_policy_patch_point_gate(
        view,
        verified.header(log),
        schema,
        admission,
        charge,
    )
    .map_err(|error| match error {
        ChargedRequestGateError::Apply(error) => Error::Apply(error),
        ChargedRequestGateError::Schema(error) => Error::Request(error.into()),
    })?;
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            let decoder_bound = complete_core_policy_patch_frame_decode_bound(
                verified.encoded(),
                schema,
                admission,
                charge,
            )
            .map_err(Error::Request)?;
            admit(sum(&[scratch(schema)?, decoder_bound])?, workspace)?;
            let decoded = consume_verified_frame(verified, schema, admission, charge)
                .map_err(Error::Request)?;
            return Ok(CompleteCorePolicyPatchPointInspection::Fresh(Box::new(
                FreshCompleteCorePolicyPatchFrame {
                    decoded,
                    log,
                    base_control,
                    control,
                    schema,
                    admission,
                    workspace_limit: workspace,
                    decoder_bound,
                },
            )));
        }
        GateDecision::Replay(original) => DispatchDecision::Replay(original),
        GateDecision::IntentConflict => DispatchDecision::IntentConflict,
        GateDecision::Expired => DispatchDecision::Expired,
        GateDecision::LogReplay => DispatchDecision::LogReplay,
    };
    let header = scaled(2048, verified.encoded().len(), 3)?;
    admit(sum(&[scratch(schema)?, header])?, workspace)?;
    drop(verified);
    Ok(CompleteCorePolicyPatchPointInspection::Complete(Box::new(
        CompleteCorePolicyPatchRequestPlan {
            expected_revision: base_control.position().get(),
            base_control,
            control,
            decision,
            retained_query: None,
            predecessor_identity: None,
            used_history_max_events_limit: None,
            used_migrations: None,
        },
    )))
}

/// Consumes Fresh owners and prepares the complete original policy response.
///
/// The returned plan owns its Fresh outcome and retains no view/engine/decoder
/// borrow. The retained lifetime is independent because no Replay is constructed.
/// # Errors
/// Admits candidate/output ownership before pure current-policy preparation and
/// preserves the same schema account through all actual work.
pub fn decide_complete_core_policy_patch_point_fresh<'retained>(
    fresh: FreshCompleteCorePolicyPatchFrame<'_>,
    view: CompleteCorePolicyPatchFreshView<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchRequestPlan<'retained>, Error> {
    profile(fresh.schema, charge)?;
    let candidate = if matches!(view.current, Ok(Some(StoredPolicyValue::Original(_)))) {
        512 + MAX_ENCODED_POLICY_BYTES
    } else {
        2048 + 216 * MAX_ENCODED_POLICY_BYTES
    };
    let required = sum(&[
        scratch(fresh.schema)?,
        fresh.decoder_bound,
        candidate,
        fresh
            .admission
            .decision_bytes()
            .min(MAX_ENCODED_POLICY_BYTES),
        1024,
    ])?;
    admit(required, fresh.workspace_limit)?;
    let intent = fresh.decoded.header(fresh.log).intent;
    let (prepared, retained_query, current, used_history_max_events_limit, used_migrations) =
        prepare_complete_core_policy_patch_point_fresh_with_query(
            view,
            fresh.decoded.into_input(),
            (intent, fresh.log, fresh.control.execution_time()),
            fresh.schema,
            fresh.admission,
            charge,
        )?;
    let predecessor_identity = capture_predecessor(current)?;
    Ok(CompleteCorePolicyPatchRequestPlan {
        expected_revision: fresh.base_control.position().get(),
        base_control: fresh.base_control,
        control: fresh.control,
        decision: DispatchDecision::Fresh(prepared),
        retained_query,
        predecessor_identity,
        used_history_max_events_limit,
        used_migrations,
    })
}

fn profile(schema: PolicySchemaLimits, charge: &PolicySchemaCharge) -> Result<(), Error> {
    if charge.profile() != schema {
        return Err(Error::Request(PolicySchemaError::ProfileMismatch.into()));
    }
    Ok(())
}

fn scratch(schema: PolicySchemaLimits) -> Result<usize, Error> {
    usize::try_from(schema.max_work_bytes()).map_err(|_| length_error())
}

fn scaled(base: usize, bytes: usize, factor: usize) -> Result<usize, Error> {
    bytes
        .checked_mul(factor)
        .and_then(|n| base.checked_add(n))
        .ok_or_else(length_error)
}

fn sum(parts: &[usize]) -> Result<usize, Error> {
    parts.iter().try_fold(0_usize, |n, next| {
        n.checked_add(*next).ok_or_else(length_error)
    })
}

fn admit(bytes: usize, maximum: usize) -> Result<(), Error> {
    if bytes > maximum {
        Err(Error::Workspace { bytes, maximum })
    } else {
        Ok(())
    }
}

fn length_error() -> Error {
    Error::Request(PolicySchemaError::LengthOverflow.into())
}
