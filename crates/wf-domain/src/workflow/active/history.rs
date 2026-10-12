//! Source event snapshots and bounded lexical history-prefix planning.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::admission::{admit_visit, charge_trace};
use super::trace::state::{TimeoutStep, TimeoutTrace, TraceValueRef};
use super::values::timeout_value_ref;
use super::{ActiveTimeoutRejection, ActiveValueRef, TimeoutError, TimeoutLimits};
use crate::claim::ScopedRecordId;
use crate::history::EventKind;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

/// Complete represented source snapshot; groups and hydrated values are not copied.
pub struct ActiveHistory {
    /// Exact decimal timestamp-version ID.
    pub event_id: String,
    /// Nondecreasing history score.
    pub event_ms: TimestampMillis,
    /// Explicit generated event kind.
    pub kind: EventKind,
    /// Committed execution time, independently of history score.
    pub occurred_at: TimestampMillis,
    /// Record component.
    pub id: RecordId,
    /// Partition component.
    pub partition: PartitionId,
    /// Actual workflow type.
    pub workflow_type: Vec<u8>,
    /// Physical state at this intermediate step.
    pub state: Vec<u8>,
    /// Intermediate record version.
    pub version: u64,
    /// Captured priority.
    pub priority: u8,
    /// Unchanged attempts.
    pub attempts: u64,
    /// Unchanged fence.
    pub fence: Fence,
    /// Original creation time.
    pub created_at: TimestampMillis,
    /// Intermediate update time.
    pub updated_at: TimestampMillis,
    /// Actual optional due time.
    pub next_run_at: Option<TimestampMillis>,
    /// Actual lease deadline, zero when absent.
    pub lease_deadline: TimestampMillis,
    /// Actual optional owner, preserving absent versus empty.
    pub worker: Option<Vec<u8>>,
    /// Actual payload reference.
    pub payload: Option<ActiveValueRef>,
    /// Actual result reference.
    pub result: Option<ActiveValueRef>,
    /// Actual intermediate error reference.
    pub error_ref: Option<ActiveValueRef>,
    /// Historical parent ID, including present-empty.
    pub parent_id: Option<Vec<u8>>,
    /// Historical root ID.
    pub root_id: Option<Vec<u8>>,
    /// Historical correlation ID.
    pub correlation_id: Option<Vec<u8>>,
    /// Historical rewind marker.
    pub rewound_to_event_id: Option<Vec<u8>>,
    /// Prepared named-reference history bytes.
    pub named_refs: Option<Vec<u8>>,
    /// Prepared attributes history bytes.
    pub attributes: Option<Vec<u8>>,
    /// Prepared state metadata history bytes.
    pub state_meta: Option<Vec<u8>>,
    /// Structured timeout metadata only for the actual timeout event.
    pub timeout: Option<TimeoutError>,
}

/// Exact score and binary-ID ordering, including same-time lexical version order.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct HistoryKey<'a> {
    /// Authoritative score.
    pub event_ms: TimestampMillis,
    /// Actual immutable event ID.
    pub event_id: &'a [u8],
}
/// One after-history deletion operation; positions are remapped before exposure.
pub struct HistoryTrim<'a> {
    /// Final write position of the affected record.
    pub owner: usize,
    /// Original authority entries deleted by this step.
    pub original: Vec<HistoryKey<'a>>,
    /// Final history positions deleted by this step.
    pub created: Vec<usize>,
}

pub(super) fn snapshot_history(
    trace: &TimeoutTrace<'_>,
    step: TimeoutStep<'_>,
) -> Result<ActiveHistory, ApplyError> {
    let original = trace.owners[step.owner].original;
    let record_core = original.core();
    let previous_ms = step
        .previous
        .map_or(record_core.updated_at(), |i| trace.steps[i].updated_at);
    let event_ms = previous_ms.max(trace.now);
    let (bytes, len) = event_id_bytes(event_ms, step.version);
    let event_id =
        String::from_utf8(bytes[..len].to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let error_ref = step.error_ref.map(|r| match r {
        TraceValueRef::Original(reference) => reference.clone(),
        TraceValueRef::Timeout { owner, version } => {
            timeout_value_ref(trace.owners[owner].original, version)
        }
    });
    Ok(ActiveHistory {
        event_id,
        event_ms,
        kind: step.kind,
        occurred_at: trace.now,
        id: record_core.id().clone(),
        partition: original.partition().clone(),
        workflow_type: record_core.workflow_type().to_vec(),
        state: step.state.to_vec(),
        version: step.version,
        priority: step.ordering.priority,
        attempts: record_core.attempts(),
        fence: record_core.fence(),
        created_at: record_core.created_at(),
        updated_at: step.updated_at,
        next_run_at: step.next_run_at,
        lease_deadline: step.lease.map_or(TimestampMillis::new(0), |l| l.deadline),
        worker: step.lease.map(|l| l.worker.clone()),
        payload: original.values().payload.clone(),
        result: original.values().result.clone(),
        error_ref,
        parent_id: original.parent().map(|p| p.id.clone()),
        root_id: original.root_id().map(<[u8]>::to_vec),
        correlation_id: original.correlation_id().map(<[u8]>::to_vec),
        rewound_to_event_id: original.carry().rewound_to_event_id.clone(),
        named_refs: original.carry().named_refs_history.clone(),
        attributes: original.carry().attributes_history.clone(),
        state_meta: original.carry().state_meta_history.clone(),
        timeout: step.timeout,
    })
}

pub(super) fn plan_history_trim(
    trace: &mut TimeoutTrace<'_>,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let mut originals = Vec::new();
    let mut counts = Vec::new();
    let mut created = Vec::new();
    for owner in &trace.owners {
        let record = owner.original;
        let id = ScopedRecordId {
            partition_key: record.partition().as_bytes(),
            id: record.core().id().as_bytes(),
        };
        admit_visit(
            &mut trace.charge,
            id.partition_key.len() + id.id.len(),
            work,
        )?;
        let keys = trace
            .view
            .history
            .get(&id)
            .ok_or(Err(ApplyError::InvalidAuthority))?;
        originals.push(keys.iter().peekable());
        counts.push(keys.len());
        created.push(BTreeSet::new());
    }
    for (position, step) in trace.steps.iter().enumerate() {
        let record = trace.owners[step.owner].original;
        let ms = step
            .previous
            .map_or(record.core().updated_at(), |i| trace.steps[i].updated_at)
            .max(trace.now);
        let (id, len) = event_id_bytes(ms, step.version);
        admit_visit(&mut trace.charge, len, work)?;
        created[step.owner].insert((ms, id, len, position));
        let mut trim = HistoryTrim {
            owner: step.owner,
            original: Vec::new(),
            created: Vec::new(),
        };
        let maximum = usize::try_from(record.captured().history_max_events())
            .map_err(|_| Err(ApplyError::InvalidAuthority))?;
        if step.version <= u64::try_from(maximum).map_err(|_| Err(ApplyError::InvalidAuthority))? {
            continue;
        }
        while counts[step.owner]
            .checked_add(created[step.owner].len())
            .ok_or(Err(ApplyError::InvalidAuthority))?
            > maximum
        {
            let old = originals[step.owner].peek().copied();
            if old.is_some_and(|k| k.event_id.len() > limits.inline_bytes()) {
                return Err(Err(ApplyError::InvalidAuthority));
            }
            let new = created[step.owner].first().copied();
            let n = old.map_or(0, |k| k.event_id.len()) + new.map_or(0, |k| k.2);
            admit_visit(&mut trace.charge, n, work)?;
            let count = trace
                .charge
                .history_deletes
                .checked_add(1)
                .ok_or(Err(ApplyError::InvalidAuthority))?;
            if count > work.max_history_deletes() {
                return Err(Ok(ActiveTimeoutRejection::HistoryDeletes {
                    count,
                    maximum: work.max_history_deletes(),
                }));
            }
            charge_trace(&mut trace.charge, 0, 1_024, work)?;
            trace.charge.history_deletes = count;
            let delete_old = match (old, new) {
                (Some(a), Some(b)) => {
                    compare_history_key((a.event_ms, a.event_id), (b.0, &b.1[..b.2]))
                        != Ordering::Greater
                }
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => return Err(Err(ApplyError::InvalidAuthority)),
            };
            if delete_old {
                let old = originals[step.owner]
                    .next()
                    .ok_or(Err(ApplyError::InvalidAuthority))?;
                counts[step.owner] -= 1;
                trim.original.push(*old);
            } else {
                let new = created[step.owner]
                    .pop_first()
                    .ok_or(Err(ApplyError::InvalidAuthority))?;
                trim.created.push(new.3);
            }
        }
        if !trim.original.is_empty() || !trim.created.is_empty() {
            trace.trims.push(trim);
        }
    }
    Ok(())
}

pub(super) fn plan_hot_evictions(
    trace: &mut TimeoutTrace<'_>,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    for step in &trace.steps {
        let original = trace.owners[step.owner].original;
        let previous = step
            .previous
            .map_or(original.core().updated_at(), |i| trace.steps[i].updated_at);
        if original.captured().history_hot_max_events() == 1 && step.version > 1 {
            admit_visit(&mut trace.charge, 0, work)?;
            trace
                .hot_evictions
                .push((step.owner, previous, step.version - 1));
        }
        if matches!(step.state, b"completed" | b"failed" | b"cancelled") {
            admit_visit(&mut trace.charge, 0, work)?;
            trace
                .hot_evictions
                .push((step.owner, previous.max(trace.now), step.version));
        }
    }
    Ok(())
}

pub(super) fn event_id_bytes(ms: TimestampMillis, version: u64) -> ([u8; 41], usize) {
    let mut bytes = [0_u8; 41];
    let mut len = 0;
    for (position, mut value) in [ms.get(), version].into_iter().enumerate() {
        if position == 1 {
            bytes[len] = b'-';
            len += 1;
        }
        let start = len;
        loop {
            bytes[len] = b'0' + u8::try_from(value % 10).unwrap_or(0);
            len += 1;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        bytes[start..len].reverse();
    }
    (bytes, len)
}

pub(super) fn compare_history_key(
    left: (TimestampMillis, &[u8]),
    right: (TimestampMillis, &[u8]),
) -> Ordering {
    left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1))
}
