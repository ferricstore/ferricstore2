//! Shared prospective represented-work accounting before any byte-owner copies.

use super::trace::state::{TimeoutStep, TimeoutTrace, TraceValueRef};
use super::{ActiveTimeoutRejection, TimeoutLimits};
use crate::claim::ScopedRecordId;
use crate::value::DomainError;
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::admission::{AdmissionError, AdmissionLimits};
use crate::workflow::batch::ApplyError;
use wf_types::bounds::MAX_RECORD_ID_BYTES;

pub(super) struct TimeoutCharge {
    pub(super) p: u64,
    pub(super) g: u64,
    pub(super) visits: u64,
    pub(super) steps: u64,
    pub(super) links: u64,
    pub(super) history_deletes: u64,
}

pub(super) fn admit_roots(
    roots: &[ScopedRecordId<'_>],
    now: u64,
    limits: AdmissionLimits,
) -> Result<TimeoutCharge, Result<ActiveTimeoutRejection, ApplyError>> {
    if roots.len() > limits.batch_items() {
        return Err(Ok(ActiveTimeoutRejection::Admission(
            AdmissionError::BatchItems,
        )));
    }
    if now > MAX_EXACT_MILLIS {
        return Err(Ok(ActiveTimeoutRejection::Domain(DomainError::NowTooLarge)));
    }
    let mut bytes = 0_usize;
    for root in roots {
        for name in [root.partition_key, root.id] {
            if name.is_empty() || name.len() > MAX_RECORD_ID_BYTES {
                return Err(Ok(ActiveTimeoutRejection::Admission(
                    AdmissionError::KeyTooLarge,
                )));
            }
            bytes = bytes
                .checked_add(name.len())
                .ok_or(Ok(ActiveTimeoutRejection::Admission(
                    AdmissionError::LengthOverflow,
                )))?;
        }
    }
    if bytes > limits.decision_bytes() {
        return Err(Ok(ActiveTimeoutRejection::Admission(
            AdmissionError::DecisionBytes,
        )));
    }
    Ok(TimeoutCharge {
        p: 0,
        g: 0,
        visits: 0,
        steps: 0,
        links: 0,
        history_deletes: 0,
    })
}

pub(super) fn charge_trace(
    charge: &mut TimeoutCharge,
    p: u64,
    g: u64,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let overflow = || {
        Ok(ActiveTimeoutRejection::Admission(
            AdmissionError::LengthOverflow,
        ))
    };
    let np = charge.p.checked_add(p).ok_or_else(overflow)?;
    let ng = charge.g.checked_add(g).ok_or_else(overflow)?;
    let total = np.checked_add(ng).ok_or_else(overflow)?;
    if total > work.max_work_bytes() {
        return Err(Ok(ActiveTimeoutRejection::WorkBytes {
            bytes: total,
            maximum: work.max_work_bytes(),
        }));
    }
    charge.p = np;
    charge.g = ng;
    Ok(())
}

pub(super) fn admit_visit(
    charge: &mut TimeoutCharge,
    bytes: usize,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let count = charge
        .visits
        .checked_add(1)
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    if count > work.max_visits() {
        return Err(Ok(ActiveTimeoutRejection::Visits {
            count,
            maximum: work.max_visits(),
        }));
    }
    let g = u64::try_from(bytes)
        .ok()
        .and_then(|n| n.checked_add(1_024))
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    charge_trace(charge, 0, g, work)?;
    charge.visits = count;
    Ok(())
}

pub(super) fn admit_group(
    charge: &mut TimeoutCharge,
    bytes: usize,
    links: usize,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let count = u64::try_from(links)
        .ok()
        .and_then(|n| charge.links.checked_add(n))
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    if count > work.max_links() {
        return Err(Ok(ActiveTimeoutRejection::Links {
            count,
            maximum: work.max_links(),
        }));
    }
    let g = u64::try_from(links)
        .ok()
        .and_then(|n| n.checked_mul(1_024))
        .and_then(|n| n.checked_add(1_024))
        .and_then(|n| u64::try_from(bytes).ok().and_then(|b| n.checked_add(b)))
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    charge_trace(charge, 0, g, work)?;
    charge.links = count;
    Ok(())
}

pub(super) fn admit_step(
    trace: &mut TimeoutTrace<'_>,
    step: TimeoutStep<'_>,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let count = trace
        .charge
        .steps
        .checked_add(1)
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    if count > work.max_steps() {
        return Err(Ok(ActiveTimeoutRejection::Steps {
            count,
            maximum: work.max_steps(),
        }));
    }
    if step.state == b"running" && step.lease.is_none_or(|lease| lease.worker.is_empty()) {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    let original = trace.owners[step.owner].original;
    let logical = if step.state == b"running" {
        original.core().run_state().unwrap_or(b"queued")
    } else {
        step.state
    };
    admit_visit(
        &mut trace.charge,
        original.core().workflow_type().len() + logical.len(),
        work,
    )?;
    let mode = trace
        .view
        .indexed
        .policies
        .get(original.core().workflow_type())
        .map_or(crate::policy::definition::StateMode::Parallel, |p| {
            p.state_mode(logical)
        });
    if mode == crate::policy::definition::StateMode::Fifo && step.ordering.priority != 0 {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    charge_trace(&mut trace.charge, 0, 1_024, work)?;
    trace.charge.steps = count;
    Ok(())
}

pub(super) fn charge_materialization(
    trace: &mut TimeoutTrace<'_>,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let mut p = 0_u64;
    for owner in &trace.owners {
        if owner.latest_step.is_none() {
            continue;
        }
        super::authority::admit_consumed_groups(owner.original, limits, &mut trace.charge, work)?;
        let r = super::record::validate_active_data(owner.original, limits)
            .map_err(|_| Err(ApplyError::InvalidAuthority))?;
        admit_visit(&mut trace.charge, r, work)?;
        p = p
            .checked_add(u64::try_from(r).map_err(|_| Err(ApplyError::InvalidAuthority))?)
            .and_then(|n| n.checked_add(2 * 1_024))
            .ok_or(Err(ApplyError::InvalidAuthority))?;
    }
    for step in &trace.steps {
        let (a, h) = step_copy_bytes(trace, step, limits).map_err(Err)?;
        admit_visit(&mut trace.charge, h + a, work)?;
        p = p
            .checked_add(u64::try_from(h + a).map_err(|_| Err(ApplyError::InvalidAuthority))?)
            .and_then(|n| n.checked_add(1_024))
            .ok_or(Err(ApplyError::InvalidAuthority))?;
    }
    for group in &trace.groups {
        if group.updates.len() > group.original.children.len() {
            return Err(Err(ApplyError::InvalidAuthority));
        }
        let owner = trace.owners[group.owner].original;
        admit_visit(
            &mut trace.charge,
            group.id.len()
                + owner.partition().as_bytes().len()
                + owner.core().id().as_bytes().len(),
            work,
        )?;
        for (id, _, result) in &group.updates {
            admit_visit(&mut trace.charge, id.as_bytes().len(), work)?;
            if let Some((_, result, error)) = result {
                for reference in [*result, *error].into_iter().flatten() {
                    let n = match reference {
                        TraceValueRef::Original(r) => super::value_ref_bytes(r, limits)
                            .map_err(|_| Err(ApplyError::InvalidAuthority))?,
                        TraceValueRef::Timeout { owner, .. } => {
                            trace.owners[owner].original.partition().as_bytes().len()
                                + trace.owners[owner].original.core().id().as_bytes().len()
                        }
                    };
                    admit_visit(&mut trace.charge, n, work)?;
                    p = p
                        .checked_add(
                            u64::try_from(n).map_err(|_| Err(ApplyError::InvalidAuthority))?,
                        )
                        .ok_or(Err(ApplyError::InvalidAuthority))?;
                }
            }
        }
    }
    charge_effect_copies(trace, work, limits, &mut p)?;
    if usize::try_from(p)
        .ok()
        .is_none_or(|n| n > limits.decision_bytes())
    {
        return Err(Ok(ActiveTimeoutRejection::Admission(
            AdmissionError::DecisionBytes,
        )));
    }
    charge_trace(&mut trace.charge, p, 0, work)
}

pub(super) fn step_copy_bytes(
    trace: &TimeoutTrace<'_>,
    step: &TimeoutStep<'_>,
    limits: AdmissionLimits,
) -> Result<(usize, usize), ApplyError> {
    let original = trace.owners[step.owner].original;
    let id = original.core().id().as_bytes().len() + original.partition().as_bytes().len();
    let mut h = id + original.core().workflow_type().len() + step.state.len();
    for value in [
        original.parent().map(|p| p.id.as_slice()),
        original.root_id(),
        original.correlation_id(),
        original.carry().rewound_to_event_id.as_deref(),
        original.carry().named_refs_history.as_deref(),
        original.carry().attributes_history.as_deref(),
        original.carry().state_meta_history.as_deref(),
        step.lease.map(|l| l.worker.as_slice()),
    ]
    .into_iter()
    .flatten()
    {
        h = h
            .checked_add(value.len())
            .ok_or(ApplyError::InvalidAuthority)?;
    }
    for reference in [
        original
            .values()
            .payload
            .as_ref()
            .map(TraceValueRef::Original),
        original
            .values()
            .result
            .as_ref()
            .map(TraceValueRef::Original),
        step.error_ref,
    ]
    .into_iter()
    .flatten()
    {
        let bytes = match reference {
            TraceValueRef::Original(r) => {
                super::value_ref_bytes(r, limits).map_err(|_| ApplyError::InvalidAuthority)?
            }
            TraceValueRef::Timeout { owner, .. } => {
                trace.owners[owner].original.partition().as_bytes().len()
                    + trace.owners[owner].original.core().id().as_bytes().len()
            }
        };
        h = h.checked_add(bytes).ok_or(ApplyError::InvalidAuthority)?;
    }
    let event_ms = step
        .previous
        .map_or(original.core().updated_at(), |s| trace.steps[s].updated_at)
        .max(trace.now);
    h = h
        .checked_add(super::history::event_id_bytes(event_ms, step.version).1)
        .ok_or(ApplyError::InvalidAuthority)?;
    let a = step.state.len() + usize::from(step.timeout.is_some()) * id;
    Ok((a, h))
}

pub(super) fn charge_effect_copies(
    trace: &mut TimeoutTrace<'_>,
    work: TimeoutLimits,
    limits: AdmissionLimits,
    p: &mut u64,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    for (owner, reservation, _) in &trace.releases {
        let original = trace.owners[*owner].original;
        let n = original.core().id().as_bytes().len()
            + original.partition().as_bytes().len()
            + reservation.scope.len()
            + reservation.reservation_id.len();
        admit_visit(&mut trace.charge, n, work)?;
        *p = p
            .checked_add(u64::try_from(n).map_err(|_| Err(ApplyError::InvalidAuthority))?)
            .and_then(|n| n.checked_add(1_024))
            .ok_or(Err(ApplyError::InvalidAuthority))?;
    }
    for (owner, _, _) in &trace.values {
        let original = trace.owners[*owner].original;
        let n = original.core().id().as_bytes().len() + original.partition().as_bytes().len();
        admit_visit(&mut trace.charge, n, work)?;
        *p = p
            .checked_add(u64::try_from(n).map_err(|_| Err(ApplyError::InvalidAuthority))?)
            .and_then(|n| n.checked_add(1_024))
            .ok_or(Err(ApplyError::InvalidAuthority))?;
    }
    let descriptors = trace
        .trims
        .len()
        .checked_add(trace.hot_evictions.len())
        .and_then(|n| n.checked_add(trace.heads.len()))
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    *p = p
        .checked_add(
            u64::try_from(descriptors)
                .ok()
                .and_then(|n| n.checked_mul(1_024))
                .ok_or(Err(ApplyError::InvalidAuthority))?,
        )
        .ok_or(Err(ApplyError::InvalidAuthority))?;

    let _ = limits;
    Ok(())
}
