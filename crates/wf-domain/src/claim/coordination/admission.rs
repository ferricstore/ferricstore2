//! Operation-local no-owner preflight, independent of source/apply admission.

use super::super::caller::NormalizedClaimDue;
use super::schedule::{ScheduleCalls, initial_reclaim_cap};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::admission::AdmissionError;
use std::fmt;

/// Immutable planning profile; every parent must supply its selected profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimCoordinationLimits {
    calls: usize,
    claim_limit: u64,
    work_bytes: u64,
}
impl Default for ClaimCoordinationLimits {
    fn default() -> Self {
        Self {
            calls: 1000,
            claim_limit: 1000,
            work_bytes: 128 * 1024 * 1024,
        }
    }
}
impl ClaimCoordinationLimits {
    /// Selects positive limits without granting caller or credit authority.
    ///
    /// # Errors
    /// Rejects zero, calls above100000 or claim limit above the exact ceiling.
    pub const fn new(
        calls: usize,
        claim_limit: u64,
        work_bytes: u64,
    ) -> Result<Self, AdmissionError> {
        if calls == 0
            || calls > 100_000
            || claim_limit == 0
            || claim_limit > MAX_EXACT_MILLIS
            || work_bytes == 0
        {
            Err(AdmissionError::InvalidLimits)
        } else {
            Ok(Self {
                calls,
                claim_limit,
                work_bytes,
            })
        }
    }
    /// Maximum original calls, independent of source filter footprint.
    #[must_use]
    pub const fn max_calls(self) -> usize {
        self.calls
    }
    /// Configured ordinary-call count maximum.
    #[must_use]
    pub const fn max_claim_limit(self) -> u64 {
        self.claim_limit
    }
    /// Logical operation-local work/descriptor ceiling, not native RSS.
    #[must_use]
    pub const fn max_work_bytes(self) -> u64 {
        self.work_bytes
    }
}
/// Pure planner failures, never automatically retained business outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimCoordinationError {
    /// Invalid positive or configured claim count.
    InvalidLimit,
    /// Ratio outside0..100.
    InvalidRatio,
    /// Empty or mixed logical-state meaning.
    InvalidStates,
    /// Invalid positive lease duration or credit quantity.
    InvalidLease,
    /// Trusted time plus caller TTL exceeds the exact ceiling.
    DeadlineOverflow,
    /// Invalid trusted time; an operational input failure.
    InvalidClock,
    /// Pipeline calls cannot carry a block attempt, including zero.
    PipelineBlockUnsupported,
    /// Partial or empty governance pair.
    InvalidGovernancePair,
    /// Governance shard outside the instance range.
    InvalidGovernanceShard,
    /// Caller semantics differ beyond their limits.
    IncompatibleCalls,
    /// Misordered, repeated or excessive phase observation.
    InvalidObservation,
    /// Empty or oversized original-call list.
    CallLimit,
    /// A represented caller component/count exceeds its profile.
    ValueTooLarge,
    /// Checked operation-local work or scalar representation exceeded.
    WorkBudget,
    /// Incoherent or over-profile stored policy value.
    InvalidPolicyAuthority,
    /// Invalid identified-credit value/observation.
    InvalidCreditAuthority,
    /// Fallible output reservation failed; an operational host failure.
    AllocationFailure,
}
impl fmt::Display for ClaimCoordinationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimit => "invalid claim planning limit",
            Self::InvalidRatio => "invalid reclaim ratio",
            Self::InvalidStates => "invalid claim planning states",
            Self::InvalidLease => "invalid claim lease or credit quantity",
            Self::DeadlineOverflow => "claim deadline exceeds exact ceiling",
            Self::InvalidClock => "invalid trusted claim time",
            Self::PipelineBlockUnsupported => "pipeline block attempt unsupported",
            Self::InvalidGovernancePair => "invalid governance scope/shard pair",
            Self::InvalidGovernanceShard => "governance shard outside instance range",
            Self::IncompatibleCalls => "incompatible claim callers",
            Self::InvalidObservation => "invalid phase observation",
            Self::CallLimit => "claim call count exceeds profile",
            Self::ValueTooLarge => "claim component exceeds planning profile",
            Self::WorkBudget => "claim planning work exceeds profile",
            Self::InvalidPolicyAuthority => "invalid running policy value",
            Self::InvalidCreditAuthority => "invalid identified credit value",
            Self::AllocationFailure => "claim planning allocation failed",
        })
    }
}
impl std::error::Error for ClaimCoordinationError {}

pub(super) fn admit_schedule_calls(
    calls: &ScheduleCalls<'_, '_>,
    limits: ClaimCoordinationLimits,
    work: &mut u64,
) -> Result<(), ClaimCoordinationError> {
    let slice = match calls {
        ScheduleCalls::Ordinary(call) => std::slice::from_ref(call),
        ScheduleCalls::Coalesced(calls) => *calls,
    };
    if slice.is_empty() || slice.len() > limits.max_calls() {
        return Err(ClaimCoordinationError::CallLimit);
    }
    charge_coordination_work(
        work,
        u64::try_from(slice.len()).map_err(|_| ClaimCoordinationError::WorkBudget)? * 1024,
        limits,
    )?;
    for call in slice {
        let counts = [
            call.states().1.len(),
            call.partitions().1.len(),
            call.values().1.len(),
        ];
        if counts.iter().any(|&n| n > 100_000) {
            return Err(ClaimCoordinationError::ValueTooLarge);
        }
        let count = counts
            .iter()
            .try_fold(0_u64, |n, &v| n.checked_add(u64::try_from(v).ok()?))
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        charge_coordination_work(work, count * 64, limits)?;
    }
    for call in slice {
        charge_coordination_work(work, coordination_call_bytes(call, limits)?, limits)?;
    }
    if matches!(calls, ScheduleCalls::Coalesced(_)) {
        let first = coordination_call_bytes(slice[0], limits)?;
        for call in &slice[1..] {
            let pair = first
                .checked_add(coordination_call_bytes(call, limits)?)
                .ok_or(ClaimCoordinationError::WorkBudget)?;
            charge_coordination_work(work, pair, limits)?;
        }
    }
    let mut total = 0_u64;
    let mut initial = 0_u64;
    for call in slice {
        let (any, states) = call.states();
        if (any && !states.is_empty())
            || (!any && states.is_empty())
            || states.iter().any(|s| s.is_empty())
        {
            return Err(ClaimCoordinationError::InvalidStates);
        }
        if call.lease_ms() == 0 || call.lease_ms() > MAX_EXACT_MILLIS {
            return Err(ClaimCoordinationError::InvalidLease);
        }
        if call.limit() == 0 || call.limit() > limits.max_claim_limit() {
            return Err(ClaimCoordinationError::InvalidLimit);
        }
        if call.reclaim_ratio() > 100 {
            return Err(ClaimCoordinationError::InvalidRatio);
        }
        if matches!(calls, ScheduleCalls::Coalesced(_)) && call.block_ms().is_some() {
            return Err(ClaimCoordinationError::PipelineBlockUnsupported);
        }
        total = total
            .checked_add(call.limit())
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        initial = initial
            .checked_add(initial_reclaim_cap(call.limit(), call.reclaim_ratio())?)
            .ok_or(ClaimCoordinationError::WorkBudget)?;
    }
    if slice.len() > 1
        && (slice[0].reclaim_ratio() == 0 || !slice[0].reclaim_expired())
        && total > limits.max_claim_limit()
    {
        return Err(ClaimCoordinationError::InvalidLimit);
    }
    Ok(())
}
pub(super) fn coordination_call_bytes(
    call: &NormalizedClaimDue<'_>,
    limits: ClaimCoordinationLimits,
) -> Result<u64, ClaimCoordinationError> {
    let fields = [call.workflow_type(), call.worker()];
    let mut bytes = 0_u64;
    for value in fields
        .into_iter()
        .chain(call.governance_limit_scope())
        .chain(call.states().1.iter().copied())
        .chain(call.partitions().1.iter().copied())
        .chain(call.values().1.iter().copied())
    {
        let count = u64::try_from(value.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?;
        bytes = bytes
            .checked_add(count)
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        if bytes > limits.max_work_bytes() {
            return Err(ClaimCoordinationError::WorkBudget);
        }
    }
    Ok(bytes)
}
pub(super) fn charge_coordination_work(
    work: &mut u64,
    added: u64,
    limits: ClaimCoordinationLimits,
) -> Result<(), ClaimCoordinationError> {
    let next = work
        .checked_add(added)
        .ok_or(ClaimCoordinationError::WorkBudget)?;
    if next > limits.max_work_bytes() {
        return Err(ClaimCoordinationError::WorkBudget);
    }
    *work = next;
    Ok(())
}
