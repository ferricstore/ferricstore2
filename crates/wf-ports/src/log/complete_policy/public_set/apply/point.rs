//! One paid point gate before original current/context and prospective owners.

use super::super::VerifiedOriginalPolicySetFrame;
use super::OriginalPolicySetRequestPlan as Plan;
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyReplaceApplyError as Existing, CompletePolicyRequestError,
    OriginalCompletePolicyReplaceApplyError as Error, OriginalPolicySetApplyView,
    OriginalPolicySetFreshView, OriginalPolicySetPreparation, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, prepare_original_policy_set_fresh_with_query,
    prepare_original_policy_set_point_fresh, restore_original_policy_set_point_frame_header,
    scan_original_policy_set_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlPlan, ControlState, PreparedDigest};
use wf_domain::workflow::request::DispatchDecision;
use wf_domain::workflow::request::gate::{
    ChargedRequestGateError, GateDecision, PointGateView,
    inspect_complete_core_policy_patch_point_gate,
};
use wf_types::version::LogIdentity;

/// Actual retained classification or Fresh-only frame ownership.
pub enum OriginalPolicySetPointInspection<'frame, 'retained> {
    /// Existing retained/control classification without current reads.
    Complete(Box<Plan<'retained>>),
    /// Original frame sealed to one actual Fresh point classification.
    Fresh(Box<FreshOriginalPolicySetFrame<'frame>>),
}

/// Original borrowed command after the real paid point gate.
pub struct FreshOriginalPolicySetFrame<'frame> {
    verified: VerifiedOriginalPolicySetFrame<'frame>,
    log: LogIdentity,
    base_control: ControlState,
    control: ControlPlan,
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
}

impl<'frame> FreshOriginalPolicySetFrame<'frame> {
    /// Borrows the original independently verified command.
    #[must_use]
    pub const fn verified(&self) -> &VerifiedOriginalPolicySetFrame<'frame> {
        &self.verified
    }
    /// Reads the exact original coherent control.
    #[must_use]
    pub const fn base_control(&self) -> ControlState {
        self.base_control
    }
    /// Reads the exact proposal from the paid gate.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
}

/// Admits header owners before full borrowed grammar, hashes and sidecar checks.
/// # Errors
/// Preserves profile, workspace, complete grammar and continuous-work refusals.
pub fn prepare_original_policy_set_point_frame_bounded<'frame>(
    encoded: &'frame [u8],
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> Result<VerifiedOriginalPolicySetFrame<'frame>, Error> {
    profile(schema, charge)?;
    admit(header_workspace(schema, encoded.len())?, workspace)?;
    let scanned = scan_original_policy_set_frame(encoded, schema, admission, charge)
        .map_err(request_error)?;
    let intent = Sha256::digest(scanned.original_intent()).into();
    // The shared scanner prospectively prices this actual PATCH hash too.
    let _patch = Sha256::digest(scanned.canonical_patch());
    let header =
        restore_original_policy_set_point_frame_header(scanned, intent, schema, admission, charge)
            .map_err(request_error)?;
    let isolation = crate::log::keys::scope([
        header.scope().tenant().as_bytes(),
        header.scope().group().as_bytes(),
        header.scope().recovery().as_bytes(),
    ])
    .map_err(|error| request_error(super::super::super::intent::map_codec_error(error)))?;
    let digest = PreparedDigest::new(Sha256::digest(encoded).into());
    Ok(VerifiedOriginalPolicySetFrame {
        header,
        digest,
        isolation,
    })
}

/// Runs one paid gate before any Fresh current, order, history or migration work.
/// # Errors
/// Preserves actual gate authority and prospective workspace/profile errors.
pub fn inspect_original_policy_set_point_frame_bounded<'frame, 'retained>(
    view: PointGateView<'_, 'retained>,
    verified: VerifiedOriginalPolicySetFrame<'frame>,
    log: LogIdentity,
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> Result<OriginalPolicySetPointInspection<'frame, 'retained>, Error> {
    profile(schema, charge)?;
    admit(
        header_workspace(schema, verified.encoded().len())?,
        workspace,
    )?;
    let base_control = view.control;
    let gate = inspect_complete_core_policy_patch_point_gate(
        view,
        verified.header(log),
        schema,
        admission,
        charge,
    )
    .map_err(|error| match error {
        ChargedRequestGateError::Apply(error) => Error::Replace(Existing::Apply(error)),
        ChargedRequestGateError::Schema(error) => request_error(error.into()),
    })?;
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => {
            return Ok(OriginalPolicySetPointInspection::Fresh(Box::new(
                FreshOriginalPolicySetFrame {
                    verified,
                    log,
                    base_control,
                    control,
                    schema,
                    admission,
                },
            )));
        }
        GateDecision::Replay(original) => DispatchDecision::Replay(original),
        GateDecision::IntentConflict => DispatchDecision::IntentConflict,
        GateDecision::Expired => DispatchDecision::Expired,
        GateDecision::LogReplay => DispatchDecision::LogReplay,
    };
    drop(verified);
    Ok(OriginalPolicySetPointInspection::Complete(Box::new(Plan {
        expected_revision: base_control.position().get(),
        base_control,
        control,
        decision,
        workflow_type: None,
        predecessor: None,
        used_history: None,
        used_migrations: None,
        portable_profile: None,
    })))
}

/// Consumes Fresh and seals actual query, predecessor and reached context.
/// # Errors
/// Admits both ETF/key/outcome owners before Fresh preparation on the same account.
pub fn decide_original_policy_set_point_fresh_bounded<'retained>(
    fresh: FreshOriginalPolicySetFrame<'_>,
    view: OriginalPolicySetFreshView<'_>,
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> Result<Plan<'retained>, Error> {
    check_fresh(&fresh, schema, admission, charge, workspace)?;
    let query = fresh
        .verified
        .header
        .copy_workflow_type(schema, charge)
        .map_err(request_error)?;
    let intent = fresh.verified.header.intent();
    let FreshOriginalPolicySetFrame {
        verified,
        log,
        base_control,
        control,
        ..
    } = fresh;
    let VerifiedOriginalPolicySetFrame {
        header, isolation, ..
    } = verified;
    drop(isolation);
    let prepared = prepare_original_policy_set_point_fresh(
        view,
        header,
        (intent, log, control.execution_time()),
        schema,
        admission,
        charge,
    )?;
    seal((base_control, control), query, prepared)
}

/// Supplies the same sealed route from a coherent mathematical Full view.
/// # Errors
/// Non-Fresh returns before supplied current/order/context, with no second gate.
pub fn decide_original_policy_set_frame_bounded<'retained>(
    view: OriginalPolicySetApplyView<'retained, '_, '_, '_>,
    encoded: &[u8],
    log: LogIdentity,
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    workspace: usize,
) -> Result<Plan<'retained>, Error> {
    let verified = prepare_original_policy_set_point_frame_bounded(
        encoded, schema, admission, charge, workspace,
    )?;
    let base = view.base();
    let outcome = base.gate.outcomes.get(verified.header.request());
    let inspected = inspect_original_policy_set_point_frame_bounded(
        PointGateView {
            scope: base.gate.scope,
            control: base.gate.control,
            retained_nonempty: !base.gate.outcomes.is_empty(),
            outcome,
        },
        verified,
        log,
        schema,
        admission,
        charge,
        workspace,
    )?;
    let fresh = match inspected {
        OriginalPolicySetPointInspection::Fresh(fresh) => fresh,
        OriginalPolicySetPointInspection::Complete(mut plan) => {
            plan.expected_revision = base.revision;
            return Ok(*plan);
        }
    };
    check_fresh(&fresh, schema, admission, charge, workspace)?;
    let query = fresh
        .verified
        .header
        .copy_workflow_type(schema, charge)
        .map_err(request_error)?;
    let intent = fresh.verified.header.intent();
    let FreshOriginalPolicySetFrame {
        verified,
        base_control,
        control,
        ..
    } = *fresh;
    let VerifiedOriginalPolicySetFrame {
        header, isolation, ..
    } = verified;
    drop(isolation);
    let prepared = prepare_original_policy_set_fresh_with_query(
        view,
        header,
        (intent, log, control.execution_time()),
        schema,
        admission,
        charge,
    )?;
    let mut plan = seal((base_control, control), query, prepared)?;
    plan.expected_revision = base.revision;
    Ok(plan)
}

fn seal<'retained>(
    controls: (ControlState, ControlPlan),
    query: Vec<u8>,
    prepared: OriginalPolicySetPreparation<'_>,
) -> Result<Plan<'retained>, Error> {
    let (fresh, current, used_history, used_migrations) = prepared;
    let predecessor = current
        .map(|raw| {
            if raw.len() > 263_168 {
                return Err(request_error(PolicySchemaError::InvalidAuthority.into()));
            }
            Ok((raw.len(), Sha256::digest(raw).into()))
        })
        .transpose()?;
    Ok(Plan {
        expected_revision: controls.0.position().get(),
        base_control: controls.0,
        control: controls.1,
        decision: DispatchDecision::Fresh(fresh),
        workflow_type: Some(query),
        predecessor,
        used_history,
        used_migrations,
        portable_profile: Some(1),
    })
}

fn check_fresh(
    fresh: &FreshOriginalPolicySetFrame<'_>,
    schema: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &PolicySchemaCharge,
    workspace: usize,
) -> Result<(), Error> {
    profile(schema, charge)?;
    if fresh.schema != schema || fresh.admission != admission {
        return Err(request_error(PolicySchemaError::InvalidAuthority.into()));
    }
    let required = header_workspace(schema, fresh.verified.encoded().len())?
        .checked_add(2 * 263_168 + 65_535 + 1024)
        .ok_or_else(overflow)?;
    admit(required, workspace)
}

fn profile(schema: PolicySchemaLimits, charge: &PolicySchemaCharge) -> Result<(), Error> {
    if charge.profile() != schema {
        return Err(request_error(PolicySchemaError::ProfileMismatch.into()));
    }
    Ok(())
}
fn header_workspace(schema: PolicySchemaLimits, encoded: usize) -> Result<usize, Error> {
    usize::try_from(schema.max_work_bytes())
        .ok()
        .and_then(|pg| pg.checked_add(65_536))
        .and_then(|n| n.checked_add(2048))
        .and_then(|n| encoded.checked_mul(3)?.checked_add(n))
        .ok_or_else(overflow)
}
fn admit(bytes: usize, maximum: usize) -> Result<(), Error> {
    if bytes > maximum {
        return Err(Error::Workspace { bytes, maximum });
    }
    Ok(())
}
fn request_error(error: CompletePolicyRequestError) -> Error {
    Error::Replace(Existing::Request(error))
}
fn overflow() -> Error {
    request_error(PolicySchemaError::LengthOverflow.into())
}
