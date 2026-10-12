//! Header-only replay gate shared by fresh record/policy/index operation dispatch.

use std::collections::BTreeMap;

use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

use super::{ClientIntent, RequestScope, RetainedOutcome};
use crate::policy::complete::{
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, charge_schema,
};
use crate::workflow::admission::{AdmissionLimits, MAX_ENCODED_ISOLATION_SCOPE_BYTES};
use crate::workflow::batch::ApplyError;
use crate::workflow::control::{
    ControlPlan, ControlState, EntryIdentity, EntryStamp, plan_control,
};

/// Immutable original request header, verified from private committed bytes.
///
/// Public requests require a trusted committed stamp. Consensus unstamped entries
/// are representable only through the separate nonrequest control API.
///
/// ```compile_fail
/// use wf_domain::workflow::request::gate::CommittedHeader;
/// use wf_domain::workflow::control::EntryStamp;
/// fn unstamped(header: &mut CommittedHeader<'_>) {
///     header.stamp = EntryStamp::Unstamped;
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct CommittedHeader<'a> {
    /// Private committed schema version, separate from public wire.
    pub format_version: u16,
    /// Original server-bound tenant/group/recovery namespace.
    pub scope: &'a RequestScope,
    /// Stable request identity in the original namespace.
    pub request: &'a RequestId,
    /// Canonical client intent, excluding generated/time inputs.
    pub intent: ClientIntent,
    /// Full log identity and separately verified prepared bytes digest.
    pub identity: EntryIdentity,
    /// Required trusted stamp; unstamped requests are unrepresentable.
    pub stamp: CommittedTime,
    /// Actual immutable entry bytes, whose field binding the decoder verifies.
    pub encoded_entry: &'a [u8],
    /// Actual isolation-scope bytes, whose typed binding the decoder verifies.
    pub encoded_isolation_scope: &'a [u8],
}

/// Coherent retained/control authority before any current operation guards.
#[derive(Clone, Copy, Debug)]
pub struct GateView<'a> {
    /// Current server-bound tenant/group/recovery namespace.
    pub scope: &'a RequestScope,
    /// Persisted time/full last-entry/checkpoint values.
    pub control: ControlState,
    /// Unique authoritative retained outcomes in this exact namespace.
    pub outcomes: &'a BTreeMap<RequestId, RetainedOutcome>,
}

/// Replay inspection result; fresh operation dispatch has not executed yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateDecision<'a> {
    /// Run the bounded operation reducer at the returned effective execution time.
    Fresh,
    /// Borrow the exact original response owner before all changed guards.
    Replay(&'a RetainedOutcome),
    /// Different canonical intent; preserve the original outcome.
    IntentConflict,
    /// Retention expired; never reexecute this still-retained identity.
    Expired,
    /// Exact last log identity/bytes/stamp; publish no metadata or operation writes.
    LogReplay,
}

/// Proposed control and gate decision; no fresh semantic outcome exists yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestGate<'a> {
    control: ControlPlan,
    decision: GateDecision<'a>,
}
impl<'a> RequestGate<'a> {
    /// Reads proposed processing time/checkpoint independently of original outcome.
    #[must_use]
    pub const fn control(self) -> ControlPlan {
        self.control
    }
    /// Reads fresh permission or borrowed original replay decision.
    #[must_use]
    pub const fn decision(self) -> GateDecision<'a> {
        self.decision
    }
}

/// Inspects scoped replay before any record, policy-generation or index guards.
///
/// Current authorization and private byte/digest verification precede this API.
/// Only `Fresh` dispatches a new bounded operation; errors leave durable authority
/// unchanged. Fresh semantic result bytes/control must later persist atomically.
///
/// # Errors
/// Rejects foreign/corrupt committed authority and conflicting log identity.
pub fn inspect_request<'view>(
    view: GateView<'view>,
    header: CommittedHeader<'_>,
    limits: AdmissionLimits,
) -> Result<RequestGate<'view>, ApplyError> {
    inspect_request_impl(view, header, limits, None).map_err(|error| match error {
        ChargedRequestGateError::Apply(error) => error,
        ChargedRequestGateError::Schema(_) => ApplyError::InvalidAuthority,
    })
}

/// Operational charged inspection refusal; it grants no execution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargedRequestGateError {
    /// Profile, checked logical arithmetic or work admission refused.
    Schema(PolicySchemaError),
    /// Header, retained or committed control authority refused.
    Apply(ApplyError),
}

impl std::fmt::Display for ChargedRequestGateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schema(error) => std::fmt::Display::fmt(error, formatter),
            Self::Apply(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for ChargedRequestGateError {}

/// Inspects one original header on the caller's continuous schema account.
///
/// Mathematical header fields do not certify private byte binding or payment.
/// The raw ports parent must freshly verify before this one inspection.
///
/// # Errors
/// Preserves profile/work/length refusal and existing operational gate priority.
pub fn inspect_complete_core_policy_patch_gate<'view>(
    view: GateView<'view>,
    header: CommittedHeader<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<RequestGate<'view>, ChargedRequestGateError> {
    if charge.profile() != limits {
        return Err(ChargedRequestGateError::Schema(
            PolicySchemaError::ProfileMismatch,
        ));
    }
    inspect_request_impl(view, header, admission, Some(charge))
}

fn inspect_request_impl<'view>(
    view: GateView<'view>,
    header: CommittedHeader<'_>,
    limits: AdmissionLimits,
    mut charge: Option<&mut PolicySchemaCharge>,
) -> Result<RequestGate<'view>, ChargedRequestGateError> {
    charge_gate(&mut charge, 128)?;
    if header.format_version != 1 {
        return Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority));
    }
    charge_gate_operands(
        &mut charge,
        &[
            view.scope.tenant().as_bytes().len(),
            view.scope.group().as_bytes().len(),
            view.scope.recovery().as_bytes().len(),
            header.scope.tenant().as_bytes().len(),
            header.scope.group().as_bytes().len(),
            header.scope.recovery().as_bytes().len(),
        ],
    )?;
    if view.scope != header.scope
        || header.encoded_entry.len() > limits.decision_bytes()
        || header.encoded_isolation_scope.len() > MAX_ENCODED_ISOLATION_SCOPE_BYTES
        || (view.control.last_entry().is_none() && !view.outcomes.is_empty())
    {
        return Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority));
    }
    charge_gate(&mut charge, 128)?;
    let control = plan_control(
        view.control,
        header.identity,
        EntryStamp::Stamped(header.stamp),
    )
    .map_err(ChargedRequestGateError::Apply)?;
    if control.is_log_replay() {
        return Ok(RequestGate {
            control,
            decision: GateDecision::LogReplay,
        });
    }
    charge_gate_operands(&mut charge, &[header.request.as_bytes().len()])?;
    let decision = if let Some(retained) = view.outcomes.get(header.request) {
        classify_retained(view, header, control, retained, limits, &mut charge)?
    } else {
        GateDecision::Fresh
    };
    Ok(RequestGate { control, decision })
}

fn classify_retained<'view>(
    view: GateView<'view>,
    header: CommittedHeader<'_>,
    control: ControlPlan,
    retained: &'view RetainedOutcome,
    limits: AdmissionLimits,
    charge: &mut Option<&mut PolicySchemaCharge>,
) -> Result<GateDecision<'view>, ChargedRequestGateError> {
    charge_gate_operands(
        charge,
        &[
            retained.scope.tenant().as_bytes().len(),
            retained.scope.group().as_bytes().len(),
            retained.scope.recovery().as_bytes().len(),
            view.scope.tenant().as_bytes().len(),
            view.scope.group().as_bytes().len(),
            view.scope.recovery().as_bytes().len(),
        ],
    )?;
    if &retained.scope != view.scope {
        return Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority));
    }
    charge_gate_operands(
        charge,
        &[
            retained.request.as_bytes().len(),
            header.request.as_bytes().len(),
        ],
    )?;
    if &retained.request != header.request {
        return Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority));
    }
    charge_gate(charge, 128)?;
    if retained.original_position.index() > view.control.position()
        || retained.original_time > view.control.time().hlc().physical()
        || retained.response.len() > limits.decision_bytes()
    {
        return Err(ChargedRequestGateError::Apply(ApplyError::InvalidAuthority));
    }
    charge_gate(charge, 196)?;
    if retained.intent != header.intent {
        return Ok(GateDecision::IntentConflict);
    }
    charge_gate(charge, 128)?;
    Ok(if control.execution_time() >= retained.expires_at {
        GateDecision::Expired
    } else {
        GateDecision::Replay(retained)
    })
}

fn charge_gate(
    charge: &mut Option<&mut PolicySchemaCharge>,
    work: u64,
) -> Result<(), ChargedRequestGateError> {
    if let Some(charge) = charge.as_deref_mut() {
        charge_schema(charge, work, 0).map_err(ChargedRequestGateError::Schema)?;
    }
    Ok(())
}

fn charge_gate_operands(
    charge: &mut Option<&mut PolicySchemaCharge>,
    lengths: &[usize],
) -> Result<(), ChargedRequestGateError> {
    if charge.is_none() {
        return Ok(());
    }
    let work = lengths.iter().try_fold(128_u64, |work, &length| {
        u64::try_from(length)
            .ok()
            .and_then(|length| work.checked_add(length))
            .ok_or(ChargedRequestGateError::Schema(
                PolicySchemaError::LengthOverflow,
            ))
    })?;
    charge_gate(charge, work)
}
