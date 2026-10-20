//! One shared point observation gate with optional continuous schema accounting.

use super::{
    AdmissionLimits, ApplyError, ChargedRequestGateError, CommittedHeader, GateDecision,
    PointGateView, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, RequestGate,
    apply_error, charge_gate_operands, classify_retained, prepare_control, validate_header,
};

/// Inspects one requested outcome and its exact scoped cohort before fresh guards.
///
/// The caller supplies coherent same-revision observations. A missing requested
/// outcome does not establish an empty cohort. Exact log replay skips retained
/// metadata only after header and cohort integrity have passed.
///
/// # Errors
/// Rejects foreign/corrupt authority, incoherent cohort or conflicting log identity.
pub fn inspect_point_request<'retained>(
    view: PointGateView<'_, 'retained>,
    header: CommittedHeader<'_>,
    limits: AdmissionLimits,
) -> Result<RequestGate<'retained>, ApplyError> {
    inspect_point_request_impl(view, header, limits, None).map_err(apply_error)
}

/// Inspects one actual requested outcome on the caller's continuous account.
///
/// The supplied facts are mathematical inputs, not proof of storage origin or
/// private-byte binding. The caller verifies once before this one gate; only
/// Fresh permits later current-policy work. Exact log replay skips the logical
/// requested-outcome observation after header and cohort integrity have passed.
///
/// # Errors
/// Preserves profile/work refusal and the existing authority/control priority.
pub fn inspect_complete_core_policy_patch_point_gate<'retained>(
    view: PointGateView<'_, 'retained>,
    header: CommittedHeader<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<RequestGate<'retained>, ChargedRequestGateError> {
    if charge.profile() != limits {
        return Err(ChargedRequestGateError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    inspect_point_request_impl(view, header, admission, Some(charge))
}

fn inspect_point_request_impl<'retained>(
    view: PointGateView<'_, 'retained>,
    header: CommittedHeader<'_>,
    limits: AdmissionLimits,
    mut charge: Option<&mut PolicySchemaCharge>,
) -> Result<RequestGate<'retained>, ChargedRequestGateError> {
    validate_header(view.scope, header, limits, &mut charge)?;
    if (view.outcome.is_some() && !view.retained_nonempty)
        || (view.control.last_entry().is_none() && view.retained_nonempty)
    {
        return Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority));
    }
    let control = prepare_control(view.control, header, &mut charge)?;
    let decision = if control.is_log_replay() {
        GateDecision::LogReplay
    } else {
        charge_gate_operands(&mut charge, &[header.request.as_bytes().len()])?;
        if let Some(retained) = view.outcome {
            classify_retained(
                view.scope,
                view.control,
                header,
                control,
                retained,
                limits,
                &mut charge,
            )?
        } else {
            GateDecision::Fresh
        }
    };
    Ok(RequestGate { control, decision })
}
