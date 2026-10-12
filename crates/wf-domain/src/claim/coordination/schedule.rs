//! Source quota planning; observations supply no workflow or credit authority.

use super::super::caller::NormalizedClaimDue;
use super::admission::{ClaimCoordinationError, ClaimCoordinationLimits, charge_coordination_work};
use super::{ClaimPhase, ClaimPhaseCompletion, ClaimScheduleStatus, ScheduledClaimPhase};

pub(super) use super::admission::admit_schedule_calls;

#[derive(Clone, Copy)]
pub(super) enum ScheduleCalls<'c, 'a> {
    Ordinary(&'c NormalizedClaimDue<'a>),
    Coalesced(&'c [&'c NormalizedClaimDue<'a>]),
}
struct CallQuota {
    limit: u64,
    initial_reclaim: u64,
    accepted: [u64; 3],
}
/// Quotas retain original caller order, never a merged request/spend identity.
pub struct ClaimSchedule<'c, 'a> {
    calls: ScheduleCalls<'c, 'a>,
    quotas: Vec<CallQuota>,
    phase_index: u8,
    status: ClaimScheduleStatus,
    limits: ClaimCoordinationLimits,
}
impl<'c, 'a> ClaimSchedule<'c, 'a> {
    /// Exposes the next actual nonzero quota, propagating checked arithmetic.
    ///
    /// # Errors
    /// Returns `WorkBudget` if the immutable scalar representation is exceeded.
    pub fn phase(&self) -> Result<Option<ScheduledClaimPhase<'c, 'a>>, ClaimCoordinationError> {
        let caller = match self.calls {
            ScheduleCalls::Ordinary(call) => call,
            ScheduleCalls::Coalesced(calls) => calls[0],
        };
        Ok(
            next_phase_quota(self)?.map(|(index, kind, quota, uses_initial_reclaim)| {
                ScheduledClaimPhase {
                    index,
                    kind,
                    quota,
                    caller,
                    uses_initial_reclaim,
                }
            }),
        )
    }
    /// Stops after completion, known failure or unknown outcome.
    #[must_use]
    pub const fn status(&self) -> ClaimScheduleStatus {
        self.status
    }
    /// Original caller index, accepted offset and count for a valid phase.
    ///
    /// Future/skipped phases contain stored zeros. Iteration is another bounded
    /// scalar pass and grants no credit/request binding for the representative.
    ///
    /// # Errors
    /// Rejects indices outside0..=2 and checked aggregate offset overflow.
    pub fn allocations(
        &self,
        phase: u8,
    ) -> Result<impl Iterator<Item = (usize, u64, u64)> + '_, ClaimCoordinationError> {
        if phase > 2 {
            return Err(ClaimCoordinationError::InvalidObservation);
        }
        self.quotas.iter().try_fold(0_u64, |offset, call| {
            offset
                .checked_add(call.accepted[usize::from(phase)])
                .ok_or(ClaimCoordinationError::WorkBudget)
        })?;
        let mut offset = 0;
        Ok(self.quotas.iter().enumerate().map(move |(index, call)| {
            let count = call.accepted[usize::from(phase)];
            let start = offset;
            offset += count; // The complete immutable sum was checked above.
            (index, start, count)
        }))
    }
}

/// Plans one ordinary call, preserving the limit-one normal-first distinction.
///
/// # Errors
/// Rejects invalid caller semantics or the selected operation-local profile.
pub fn plan_claim_schedule<'c, 'a>(
    call: &'c NormalizedClaimDue<'a>,
    limits: ClaimCoordinationLimits,
) -> Result<ClaimSchedule<'c, 'a>, ClaimCoordinationError> {
    let calls = ScheduleCalls::Ordinary(call);
    let mut work = 0;
    charge_coordination_work(&mut work, 64, limits)?;
    admit_schedule_calls(&calls, limits, &mut work)?;
    let mut quotas = Vec::new();
    quotas
        .try_reserve(1)
        .map_err(|_| ClaimCoordinationError::AllocationFailure)?;
    quotas.push(CallQuota {
        limit: call.limit(),
        initial_reclaim: initial_reclaim_cap(call.limit(), call.reclaim_ratio())?,
        accepted: [0; 3],
    });
    Ok(ClaimSchedule {
        calls,
        quotas,
        phase_index: 0,
        status: ClaimScheduleStatus::Ready,
        limits,
    })
}

/// Plans compatible calls with separately rounded caps and original allocations.
///
/// The first caller is only a filter representative. Every original caller and
/// allocation index must remain independently bound by the future parent.
///
/// # Errors
/// Rejects empty, blocked, incompatible or over-profile calls before ownership.
pub fn plan_coalesced_claim_schedule<'c, 'a>(
    calls: &'c [&'c NormalizedClaimDue<'a>],
    limits: ClaimCoordinationLimits,
) -> Result<ClaimSchedule<'c, 'a>, ClaimCoordinationError> {
    let inputs = ScheduleCalls::Coalesced(calls);
    let mut work = 0;
    let count = u64::try_from(calls.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?;
    charge_coordination_work(
        &mut work,
        count
            .checked_mul(64)
            .ok_or(ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    admit_schedule_calls(&inputs, limits, &mut work)?;
    if calls[1..]
        .iter()
        .any(|call| !compatible_calls(calls[0], call))
    {
        return Err(ClaimCoordinationError::IncompatibleCalls);
    }
    let mut quotas = Vec::new();
    quotas
        .try_reserve(calls.len())
        .map_err(|_| ClaimCoordinationError::AllocationFailure)?;
    for call in calls {
        quotas.push(CallQuota {
            limit: call.limit(),
            initial_reclaim: initial_reclaim_cap(call.limit(), call.reclaim_ratio())?,
            accepted: [0; 3],
        });
    }
    Ok(ClaimSchedule {
        calls: if calls.len() == 1 {
            ScheduleCalls::Ordinary(calls[0])
        } else {
            inputs
        },
        quotas,
        phase_index: 0,
        status: ClaimScheduleStatus::Ready,
        limits,
    })
}

/// Advances only the exact exposed phase, atomically updating scalar counts.
///
/// # Errors
/// Misordered/repeated/excessive observations leave the schedule unchanged.
pub fn advance_claim_schedule(
    schedule: &mut ClaimSchedule<'_, '_>,
    completion: ClaimPhaseCompletion,
) -> Result<(), ClaimCoordinationError> {
    let current = schedule
        .phase()?
        .ok_or(ClaimCoordinationError::InvalidObservation)?;
    let phase = match completion {
        ClaimPhaseCompletion::Accepted { phase, .. }
        | ClaimPhaseCompletion::KnownFailure { phase }
        | ClaimPhaseCompletion::UnknownOutcome { phase } => phase,
    };
    if phase != current.index {
        return Err(ClaimCoordinationError::InvalidObservation);
    }
    let quota = current.quota;
    let initial = current.uses_initial_reclaim;
    match completion {
        ClaimPhaseCompletion::Accepted { claimed, .. } => {
            if claimed > quota {
                return Err(ClaimCoordinationError::InvalidObservation);
            }
            let next = phase
                .checked_add(1)
                .ok_or(ClaimCoordinationError::WorkBudget)?;
            allocate_phase_count(&mut schedule.quotas, phase, initial, claimed)?;
            schedule.phase_index = next;
            if next_phase_quota(schedule)?.is_none() {
                schedule.status = ClaimScheduleStatus::Complete;
            }
        }
        ClaimPhaseCompletion::KnownFailure { .. } => {
            schedule.status = ClaimScheduleStatus::KnownFailure;
        }
        ClaimPhaseCompletion::UnknownOutcome { .. } => {
            schedule.status = ClaimScheduleStatus::UnknownOutcome;
        }
    }
    Ok(())
}

fn compatible_calls(left: &NormalizedClaimDue<'_>, right: &NormalizedClaimDue<'_>) -> bool {
    left.workflow_type() == right.workflow_type()
        && left.worker() == right.worker()
        && left.states() == right.states()
        && left.partitions() == right.partitions()
        && left.values() == right.values()
        && left.lease_ms() == right.lease_ms()
        && left.priority() == right.priority()
        && left.reclaim_expired() == right.reclaim_expired()
        && left.reclaim_ratio() == right.reclaim_ratio()
        && left.reply_mode() == right.reply_mode()
        && left.payload_enabled() == right.payload_enabled()
        && left.payload_max_bytes() == right.payload_max_bytes()
        && left.governance_limit_scope() == right.governance_limit_scope()
        && left.governance_shard_id() == right.governance_shard_id()
        && left.requested_now_ms() == right.requested_now_ms()
}
pub(super) fn initial_reclaim_cap(limit: u64, ratio: u64) -> Result<u64, ClaimCoordinationError> {
    let product = limit
        .checked_mul(ratio)
        .ok_or(ClaimCoordinationError::WorkBudget)?;
    Ok(product
        .checked_add(99)
        .ok_or(ClaimCoordinationError::WorkBudget)?
        .checked_div(100)
        .ok_or(ClaimCoordinationError::WorkBudget)?
        .max(1))
}
fn next_phase_quota(
    schedule: &ClaimSchedule<'_, '_>,
) -> Result<Option<(u8, ClaimPhase, u64, bool)>, ClaimCoordinationError> {
    if schedule.quotas.len() > schedule.limits.max_calls() {
        return Err(ClaimCoordinationError::CallLimit);
    }
    if schedule.status != ClaimScheduleStatus::Ready {
        return Ok(None);
    }
    let (call, ordinary) = match schedule.calls {
        ScheduleCalls::Ordinary(call) => (call, true),
        ScheduleCalls::Coalesced(calls) => (calls[0], false),
    };
    let direct = call.states() == (false, &[b"running".as_slice()]);
    let one_phase = direct || !call.reclaim_expired() || call.reclaim_ratio() == 0;
    let normal_first = ordinary && call.limit() == 1 && !one_phase;
    for phase in schedule.phase_index..=2 {
        if (one_phase && phase > 0) || (normal_first && phase > 1) {
            return Ok(None);
        }
        let initial = !one_phase && !normal_first && phase == 0;
        let quota = schedule.quotas.iter().try_fold(0_u64, |total, item| {
            let amount = if initial {
                item.initial_reclaim
            } else {
                let used = item
                    .accepted
                    .iter()
                    .try_fold(0_u64, |n, v| n.checked_add(*v))
                    .ok_or(ClaimCoordinationError::WorkBudget)?;
                item.limit
                    .checked_sub(used)
                    .ok_or(ClaimCoordinationError::WorkBudget)?
            };
            total
                .checked_add(amount)
                .ok_or(ClaimCoordinationError::WorkBudget)
        })?;
        let kind = if direct
            || (!one_phase && ((normal_first && phase == 1) || (!normal_first && phase != 1)))
        {
            ClaimPhase::Running
        } else if !call.reclaim_expired() || !one_phase {
            ClaimPhase::Normal
        } else {
            ClaimPhase::Requested
        };
        if quota > 0 {
            return Ok(Some((phase, kind, quota, initial)));
        }
    }
    Ok(None)
}
fn allocate_phase_count(
    quotas: &mut [CallQuota],
    phase: u8,
    initial: bool,
    claimed: u64,
) -> Result<(), ClaimCoordinationError> {
    if phase > 2 {
        return Err(ClaimCoordinationError::InvalidObservation);
    }
    let mut total = 0_u64;
    for item in quotas.iter() {
        let used = item
            .accepted
            .iter()
            .try_fold(0_u64, |n, value| n.checked_add(*value))
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        let remaining = item
            .limit
            .checked_sub(used)
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        let capacity = if initial {
            item.initial_reclaim
        } else {
            remaining
        };
        total = total
            .checked_add(capacity)
            .ok_or(ClaimCoordinationError::WorkBudget)?;
    }
    if claimed > total {
        return Err(ClaimCoordinationError::InvalidObservation);
    }
    let mut remaining = claimed;
    for item in quotas {
        let capacity = if initial {
            item.initial_reclaim
        } else {
            item.limit - item.accepted.iter().sum::<u64>()
        };
        let count = remaining.min(capacity);
        item.accepted[usize::from(phase)] = count;
        remaining -= count; // Complete prospective arithmetic was validated above.
    }
    Ok(())
}
