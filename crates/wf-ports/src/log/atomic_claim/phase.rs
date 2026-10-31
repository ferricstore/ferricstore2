//! One original parent gate and derived phase envelopes with actual root identity.

use super::super::{atomic_claim_retained_summary, writer::Writer};
use super::{CodecError as E, CodecLimits, PreparedAtomicClaim, caller};
use wf_domain::{
    claim::{
        PartitionFilter,
        apply::{ClaimEnvelope, ClaimQuery, ClaimRejection, ClaimStates},
        caller::ClaimPartitionMode,
        coordination::{ClaimPhase, ScheduledClaimPhase},
    },
    workflow::{
        control::ControlPlan,
        request::gate::{GateDecision, PointGateView, RequestGate, inspect_point_request},
    },
};
use wf_types::version::LogIdentity;

/// Existing original classification or the exact prepared parent after its gate.
pub enum AtomicClaimInspection<'p, 'input, 'retained> {
    /// Retained, conflict, expiry or exact-log classification with processing control.
    Complete(RequestGate<'retained>),
    /// Original actual parent after one real Fresh inspection.
    Fresh(FreshAtomicClaim<'p, 'input>),
}

/// Actual root capture and its single point-gate control; never a leaf projection.
pub struct FreshAtomicClaim<'p, 'input> {
    prepared: &'p PreparedAtomicClaim<'input>,
    control: ControlPlan,
    log: LogIdentity,
}
impl<'p, 'input> FreshAtomicClaim<'p, 'input> {
    /// Original completely prepared immutable parent.
    #[must_use]
    pub const fn prepared(&self) -> &'p PreparedAtomicClaim<'input> {
        self.prepared
    }
    /// Actual processing control/time from the sole retained gate.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }

    /// Derives a source-scheduled phase while retaining the actual root header.
    /// Progress contains previously measured scans, heads and written ordinals.
    /// # Errors
    /// Refuses changed caller bytes, unresolved source routes or inconsistent progress;
    /// numeric query rejection remains distinct inside the successful result.
    pub fn envelope<'s>(
        &'s self,
        phase: &'s ScheduledClaimPhase<'_, '_>,
        progress: (usize, usize, u32),
        limits: CodecLimits,
    ) -> Result<Result<ClaimEnvelope<'s>, ClaimRejection>, E> {
        let call = phase.caller();
        let mut check = Writer::verify(
            self.prepared.caller_bytes,
            limits.admission().decision_bytes(),
        )?;
        caller::write(&mut check, call, limits)?;
        check.finish_verification()?;
        let (mode, keys) = call.partitions();
        let partitions = match mode {
            ClaimPartitionMode::Any => PartitionFilter::Any,
            ClaimPartitionMode::Only => PartitionFilter::Only(keys),
            ClaimPartitionMode::Auto | ClaimPartitionMode::Global => return Err(E::UnsupportedTag),
        };
        let priority = match call.priority() {
            Some(n) if n > 2 => return Ok(Err(ClaimRejection::InvalidPriority)),
            Some(n) => Some(u8::try_from(n).map_err(|_| E::Malformed)?),
            None => None,
        };
        let Ok(quota) = usize::try_from(phase.quota()) else {
            return Ok(Err(ClaimRejection::InvalidBudget));
        };
        let (any, names) = call.states();
        let mut states = if any {
            ClaimStates::Any
        } else {
            ClaimStates::Only(names)
        };
        let mut exclude_states: &[&[u8]] = &[];
        match phase.kind() {
            ClaimPhase::Requested => {}
            ClaimPhase::Normal => exclude_states = &[b"running"],
            ClaimPhase::Running => states = ClaimStates::Only(&[b"running"]),
        }
        let scan_budget = self
            .prepared
            .scan_budget
            .checked_sub(progress.0)
            .ok_or(E::Malformed)?;
        let head_budget = self
            .prepared
            .head_budget
            .checked_sub(progress.1)
            .ok_or(E::Malformed)?;
        let Some(first_ordinal) = self.prepared.first_ordinal.checked_add(progress.2) else {
            return Ok(Err(ClaimRejection::LengthOverflow));
        };
        Ok(Ok(ClaimEnvelope {
            header: self.prepared.header(self.log),
            query: ClaimQuery {
                workflow_type: call.workflow_type(),
                partitions,
                states,
                exclude_states,
                priority,
                worker: call.worker(),
                lease_ms: call.lease_ms(),
                limit: quota,
            },
            token_version: self.prepared.token_version,
            first_ordinal,
            scan_budget,
            head_budget,
            resume: None,
        }))
    }
}

/// Inspects actual root bytes once, validating original retained data before Fresh.
/// # Errors
/// Refuses header/scope/control inconsistency or malformed original parent receipts.
pub fn inspect_atomic_claim_point<'p, 'input, 'retained>(
    prepared: &'p PreparedAtomicClaim<'input>,
    log: LogIdentity,
    view: PointGateView<'_, 'retained>,
    limits: CodecLimits,
) -> Result<AtomicClaimInspection<'p, 'input, 'retained>, super::super::RecordBatchApplyError> {
    let gate = inspect_point_request(view, prepared.header(log), limits.admission())?;
    if let GateDecision::Replay(outcome) = gate.decision() {
        let summary = if outcome.intent().version() == 21 {
            crate::log::public_claim_retained_summary
        } else {
            atomic_claim_retained_summary
        };
        let (status, _) = summary(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
            outcome.original_time(),
            limits.admission(),
        )?;
        if status != outcome.status() {
            return Err(E::Malformed.into());
        }
    }
    Ok(if gate.decision() == GateDecision::Fresh {
        AtomicClaimInspection::Fresh(FreshAtomicClaim {
            prepared,
            control: gate.control(),
            log,
        })
    } else {
        AtomicClaimInspection::Complete(gate)
    })
}
