//! Apply-private bounded scalar/reference overlay; no rich owner is cloned here.

use super::super::admission::TimeoutCharge;
use super::super::admission::admit_visit;
use super::super::indexes::PlannedFifoHead;
use super::super::{
    ActiveRecord, ActiveTimeoutRejection, ActiveTimeoutView, ActiveValueRef, GovernanceReservation,
    HistoryTrim, ReleaseOutboxState, TimeoutError, TimeoutLimits,
};
use crate::claim::ScopedRecordId;
use crate::history::EventKind;
use crate::lease::Lease;
use crate::lineage::{ChildGroup, ChildResolution, ChildStatus, ChildSummary};
use crate::workflow::batch::ApplyError;
use crate::workflow::indexed::{EntryOrder, OrderingMetadata};
use std::collections::{BTreeMap, BTreeSet};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

#[derive(Clone, Copy)]
pub(in super::super) enum TraceValueRef<'v> {
    Original(&'v ActiveValueRef),
    Timeout { owner: usize, version: u64 },
}
pub(in super::super) struct ShadowRecord<'v> {
    pub(in super::super) original: &'v ActiveRecord,
    pub(in super::super) latest_step: Option<usize>,
    pub(in super::super) group_updates: Vec<usize>,
}
pub(in super::super) type ShadowChildUpdate<'v> = (
    &'v RecordId,
    ChildStatus,
    Option<(
        ChildStatus,
        Option<TraceValueRef<'v>>,
        Option<TraceValueRef<'v>>,
    )>,
);
pub(in super::super) struct ShadowGroup<'v> {
    pub(in super::super) owner: usize,
    pub(in super::super) id: &'v [u8],
    pub(in super::super) original: &'v ChildGroup,
    pub(in super::super) prior: Option<usize>,
    pub(in super::super) summary: ChildSummary,
    pub(in super::super) resolved: Option<ChildResolution>,
    pub(in super::super) updates: Vec<ShadowChildUpdate<'v>>,
}
#[derive(Clone, Copy)]
pub(in super::super) struct TimeoutStep<'v> {
    pub(in super::super) owner: usize,
    pub(in super::super) previous: Option<usize>,
    pub(in super::super) group_checkpoint: usize,
    pub(in super::super) kind: EventKind,
    pub(in super::super) version: u64,
    pub(in super::super) state: &'v [u8],
    pub(in super::super) updated_at: TimestampMillis,
    pub(in super::super) next_run_at: Option<TimestampMillis>,
    pub(in super::super) lease: Option<&'v Lease>,
    pub(in super::super) ttl_ms: Option<u64>,
    pub(in super::super) retention_until: Option<TimestampMillis>,
    pub(in super::super) error_ref: Option<TraceValueRef<'v>>,
    pub(in super::super) timeout: Option<TimeoutError>,
    pub(in super::super) ordering: OrderingMetadata,
    pub(in super::super) held_reservation: Option<&'v GovernanceReservation>,
}
pub(in super::super) enum TimeoutFrame {
    ExpireRoot {
        owner: usize,
    },
    CancelClosed {
        owner: usize,
        held_step: usize,
    },
    CancelChild {
        owner: usize,
    },
    FinishCancelledGroups {
        owner: usize,
        held_step: usize,
        patches: Vec<usize>,
    },
    NotifyParent {
        child: usize,
        held_step: usize,
        status: ChildStatus,
    },
    PropagateParent {
        owner: usize,
        held_step: usize,
    },
    LeavePath {
        owner: usize,
    },
}
pub(in super::super) struct TimeoutTrace<'v> {
    pub(in super::super) view: ActiveTimeoutView<'v>,
    pub(in super::super) now: TimestampMillis,
    pub(in super::super) order: EntryOrder,
    pub(in super::super) owners: Vec<ShadowRecord<'v>>,
    pub(in super::super) lookup: BTreeMap<ScopedRecordId<'v>, usize>,
    pub(in super::super) groups: Vec<ShadowGroup<'v>>,
    pub(in super::super) steps: Vec<TimeoutStep<'v>>,
    pub(in super::super) frames: Vec<TimeoutFrame>,
    pub(in super::super) path: BTreeSet<ScopedRecordId<'v>>,
    pub(in super::super) trims: Vec<HistoryTrim<'v>>,
    pub(in super::super) hot_evictions: Vec<(usize, TimestampMillis, u64)>,
    pub(in super::super) heads: Vec<PlannedFifoHead<'v>>,
    pub(in super::super) values: Vec<(usize, u64, u64)>,
    pub(in super::super) releases: Vec<(usize, &'v GovernanceReservation, TimestampMillis)>,
    pub(in super::super) expected_outbox: ReleaseOutboxState,
    pub(in super::super) next_outbox: ReleaseOutboxState,
    pub(in super::super) expired_roots: u64,
    pub(in super::super) next_ordinal: Option<u32>,
    pub(in super::super) consumed_ordinals: u64,
    pub(in super::super) charge: TimeoutCharge,
}

pub(in super::super) fn current_group(
    trace: &mut TimeoutTrace<'_>,
    owner: usize,
    id: &[u8],
    checkpoint: usize,
    work: TimeoutLimits,
) -> Result<Option<usize>, Result<ActiveTimeoutRejection, ApplyError>> {
    for position in trace.owners[owner].group_updates[..checkpoint].iter().rev() {
        let candidate = trace.groups[*position].id;
        let bytes = candidate
            .len()
            .checked_add(id.len())
            .ok_or(Err(ApplyError::InvalidAuthority))?;
        admit_visit(&mut trace.charge, bytes, work)?;
        if candidate == id {
            return Ok(Some(*position));
        }
    }
    Ok(None)
}

pub(in super::super) fn current_child_status(
    trace: &mut TimeoutTrace<'_>,
    mut prior: Option<usize>,
    child: &RecordId,
    original: ChildStatus,
    work: TimeoutLimits,
) -> Result<ChildStatus, Result<ActiveTimeoutRejection, ApplyError>> {
    while let Some(i) = prior {
        for (candidate, status, _) in trace.groups[i].updates.iter().rev() {
            let bytes = candidate
                .as_bytes()
                .len()
                .checked_add(child.as_bytes().len())
                .ok_or(Err(ApplyError::InvalidAuthority))?;
            admit_visit(&mut trace.charge, bytes, work)?;
            if *candidate == child {
                return Ok(*status);
            }
        }
        prior = trace.groups[i].prior;
    }
    Ok(original)
}
