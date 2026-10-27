//! Final owners and borrowed descriptions from one accepted immutable authority.

use super::ActiveTimeoutRejection;
use super::TimeoutLimits;
use super::admission::admit_visit;
use super::history::snapshot_history;
use super::indexes::{ActiveIndexChange, PlannedFifoHead};
use super::trace::state::{TimeoutStep, TimeoutTrace, TraceValueRef};
use super::values::timeout_value_ref;
use super::{
    ActiveHistory, ActiveIndexDelta, ActiveRecord, ActiveTimeoutView, HistoryTrim, ReleaseAppend,
    ReleaseIntent, ReleaseOutboxState, TimeoutError, TimeoutValueWrite, index_deltas,
};
use crate::claim::{ClaimRecord, LaneKey, ScopedRecordId};
use crate::history::LatestEvent;
use crate::lineage::ChildResult;
use crate::policy::definition::StateMode;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use wf_types::version::TimestampMillis;

/// One final touched write, guarded by the ORIGINAL authoritative version.
#[derive(Clone)]
pub struct ActiveRecordWrite {
    /// Version before every intermediate step in this entry.
    pub expected_version: u64,
    /// Final owner after all intermediate history-producing steps.
    pub record: ActiveRecord,
}

/// Sealed pure write plan; the original view and this owner must survive encoding.
pub struct ActiveTimeoutPlan<'v> {
    view: ActiveTimeoutView<'v>,
    writes: Vec<ActiveRecordWrite>,
    history: Vec<ActiveHistory>,
    values: Vec<TimeoutValueWrite>,
    trims: Vec<HistoryTrim<'v>>,
    hot_evictions: Vec<(usize, TimestampMillis, u64)>,
    changes: Vec<ActiveIndexChange<'v>>,
    heads: Vec<PlannedFifoHead<'v>>,
    releases: Vec<ReleaseAppend>,
    expected_outbox: ReleaseOutboxState,
    next_outbox: ReleaseOutboxState,
    expired_roots: u64,
    consumed_ordinals: u64,
    next_ordinal: Option<u32>,
    p: u64,
    g: u64,
}

impl<'v> ActiveTimeoutPlan<'v> {
    /// Refetches a final touched owner or the same original accepted authority.
    #[must_use]
    pub fn record<'b>(&'b self, id: ScopedRecordId<'_>) -> Option<&'b ActiveRecord>
    where
        'v: 'b,
    {
        self.writes
            .iter()
            .find(|w| {
                w.record.partition().as_bytes() == id.partition_key
                    && w.record.core().id().as_bytes() == id.id
            })
            .map(|w| &w.record)
            .or_else(|| self.view.records.get(&id).copied())
    }
    /// Derives short-borrow after rows; no record equality or substituted lookup.
    pub fn deltas<'b>(
        &'b self,
    ) -> impl Iterator<Item = Result<ActiveIndexDelta<'b>, ApplyError>> + 'b
    where
        'v: 'b,
    {
        self.changes.iter().map(|c| {
            index_deltas(
                c.before,
                &self.writes[c.write].record,
                c.before_mode,
                c.after_mode,
            )
        })
    }
    /// Returns preplanned original/final heads, including an unscheduled blocker.
    pub fn affected_fifo_heads<'b>(
        &'b self,
    ) -> impl Iterator<Item = (LaneKey<'b>, Option<ClaimRecord<'b>>)> + 'b
    where
        'v: 'b,
    {
        self.heads.iter().map(|(lane, head)| {
            (
                *lane,
                head.map(|(write, before)| {
                    let record = write.map_or(before, |i| &self.writes[i].record);
                    ClaimRecord {
                        record: record.core(),
                        partition_key: record.partition().as_bytes(),
                        priority: record.ordering().priority,
                        state_enter_seq: record.ordering().state_enter_seq,
                    }
                }),
            )
        })
    }
    /// Borrows final writes in first-mutation order.
    #[must_use]
    pub fn writes(&self) -> &[ActiveRecordWrite] {
        &self.writes
    }
    /// Borrows every intermediate snapshot in source step order.
    #[must_use]
    pub fn history(&self) -> &[ActiveHistory] {
        &self.history
    }
    /// Borrows the separately owned new error identities and bodies.
    #[must_use]
    pub fn value_writes(&self) -> &[TimeoutValueWrite] {
        &self.values
    }
    /// Borrows history trims with final write/history positions.
    #[must_use]
    pub fn trims(&self) -> &[HistoryTrim<'v>] {
        &self.trims
    }
    /// Derives admitted hot eviction IDs without another byte owner.
    pub fn hot_evictions(
        &self,
    ) -> impl Iterator<Item = (ScopedRecordId<'_>, TimestampMillis, u64)> {
        self.hot_evictions.iter().map(|(write, ms, version)| {
            let record = &self.writes[*write].record;
            (
                ScopedRecordId {
                    partition_key: record.partition().as_bytes(),
                    id: record.core().id().as_bytes(),
                },
                *ms,
                *version,
            )
        })
    }
    /// Borrows ordered release append intents; this is not a remote acknowledgement.
    #[must_use]
    pub fn releases(&self) -> &[ReleaseAppend] {
        &self.releases
    }
    /// Returns expected and next complete outbox scalars.
    #[must_use]
    pub const fn outbox(&self) -> (ReleaseOutboxState, ReleaseOutboxState) {
        (self.expected_outbox, self.next_outbox)
    }
    /// Counts actually expired explicit roots, excluding child cancellations.
    #[must_use]
    pub const fn stats(&self) -> u64 {
        self.expired_roots
    }
    /// Counts allocated logical state-enter offsets.
    #[must_use]
    pub const fn consumed_ordinals(&self) -> u64 {
        self.consumed_ordinals
    }
    /// Returns the next offset, or None after consuming the maximum offset.
    #[must_use]
    pub const fn next_ordinal(&self) -> Option<u32> {
        self.next_ordinal
    }
    /// Returns admitted copied-owner bytes and cumulative represented-work bytes.
    #[must_use]
    pub const fn charges(&self) -> (u64, u64) {
        (self.p, self.g)
    }
}

pub(super) fn materialize_trace(
    mut trace: TimeoutTrace<'_>,
    _limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<ActiveTimeoutPlan<'_>, Result<ActiveTimeoutRejection, ApplyError>> {
    let invalid = || Err(ApplyError::InvalidAuthority);
    for _ in &trace.owners {
        admit_visit(&mut trace.charge, 0, work)?;
    }
    let mut owner_to_write = vec![None; trace.owners.len()];
    let mut step_to_history = Vec::new();
    let mut owners = Vec::new();
    for step in &trace.steps {
        admit_visit(&mut trace.charge, 0, work)?;
        if owner_to_write[step.owner].is_none() {
            owner_to_write[step.owner] = Some(owners.len());
            owners.push(step.owner);
        }
        step_to_history.push(step_to_history.len());
    }
    for trim in &mut trace.trims {
        admit_visit(&mut trace.charge, 0, work)?;
        trim.owner = owner_to_write[trim.owner].ok_or_else(invalid)?;
        for created in &mut trim.created {
            admit_visit(&mut trace.charge, 0, work)?;
            *created = step_to_history[*created];
        }
    }
    for (owner, _, _) in &mut trace.hot_evictions {
        admit_visit(&mut trace.charge, 0, work)?;
        *owner = owner_to_write[*owner].ok_or_else(invalid)?;
    }
    for (_, head) in &mut trace.heads {
        admit_visit(&mut trace.charge, 0, work)?;
        if let Some((Some(owner), _)) = head {
            *owner = owner_to_write[*owner].ok_or_else(invalid)?;
        }
    }
    let mut changes = Vec::new();
    let mut writes = Vec::new();
    let policies = trace.view.indexed.policies;
    for owner in owners {
        let before = trace.owners[owner].original;
        let final_step = trace.steps[trace.owners[owner].latest_step.ok_or_else(invalid)?];
        let before_state = if before.core().state() == b"running" {
            before.core().run_state().unwrap_or(b"queued")
        } else {
            before.core().state()
        };
        let after_state = if final_step.state == b"running" {
            before.core().run_state().unwrap_or(b"queued")
        } else {
            final_step.state
        };
        for key in [before.core().workflow_type(), before_state, after_state] {
            admit_visit(&mut trace.charge, key.len(), work)?;
        }
        let policy = policies.get(before.core().workflow_type());
        changes.push(ActiveIndexChange {
            before,
            write: changes.len(),
            before_mode: policy.map_or(StateMode::Parallel, |p| p.state_mode(before_state)),
            after_mode: policy.map_or(StateMode::Parallel, |p| p.state_mode(after_state)),
        });
    }
    for change in &changes {
        let before = change.before;
        writes.push(ActiveRecordWrite {
            expected_version: before.core().version(),
            record: before.clone(),
        });
    }
    let mut history = Vec::new();
    for step in &trace.steps {
        let write = owner_to_write[step.owner].ok_or_else(invalid)?;
        apply_shadow(&trace, *step, &mut writes[write].record).map_err(Err)?;
        history.push(snapshot_history(&trace, *step).map_err(Err)?);
    }
    let (values, releases) = materialize_effects(&trace).map_err(Err)?;
    Ok(ActiveTimeoutPlan {
        view: trace.view,
        writes,
        history,
        values,
        trims: trace.trims,
        hot_evictions: trace.hot_evictions,
        changes,
        heads: trace.heads,
        releases,
        expected_outbox: trace.expected_outbox,
        next_outbox: trace.next_outbox,
        expired_roots: trace.expired_roots,
        consumed_ordinals: trace.consumed_ordinals,
        next_ordinal: trace.next_ordinal,
        p: trace.charge.p,
        g: trace.charge.g,
    })
}

fn materialize_effects(
    trace: &TimeoutTrace<'_>,
) -> Result<(Vec<TimeoutValueWrite>, Vec<ReleaseAppend>), ApplyError> {
    let values = trace
        .values
        .iter()
        .map(|(owner, version, duration)| TimeoutValueWrite {
            reference: timeout_value_ref(trace.owners[*owner].original, *version),
            body: TimeoutError {
                max_active_ms: *duration,
            },
            expires_at: Some(TimestampMillis::new(
                trace
                    .now
                    .get()
                    .saturating_add(trace.owners[*owner].original.captured().retention_ttl_ms())
                    .min(crate::workflow::MAX_EXACT_MILLIS),
            )),
        })
        .collect();
    let mut releases = Vec::new();
    for (offset, (owner, reservation, created_at)) in trace.releases.iter().enumerate() {
        let original = trace.owners[*owner].original;
        let sequence = trace
            .expected_outbox
            .tail
            .checked_add(u64::try_from(offset).map_err(|_| ApplyError::InvalidAuthority)? + 1)
            .ok_or(ApplyError::InvalidAuthority)?;
        releases.push(ReleaseAppend {
            sequence,
            intent: ReleaseIntent {
                flow_id: original.core().id().clone(),
                partition: original.partition().clone(),
                scope: reservation.scope.clone(),
                shard_id: reservation.shard_id,
                reservation_id: reservation.reservation_id.clone(),
                enforcement: reservation.enforcement,
                created_at: *created_at,
            },
        });
    }
    Ok((values, releases))
}

pub(super) fn apply_shadow(
    trace: &TimeoutTrace<'_>,
    step: TimeoutStep<'_>,
    record: &mut ActiveRecord,
) -> Result<(), ApplyError> {
    record.core.state = step.state.to_vec();
    record.core.version = step.version;
    record.core.updated_at = step.updated_at;
    record.core.next_run_at = step.next_run_at;
    if step.lease.is_none() {
        record.core.lease = None;
    }
    if step.timeout.is_some() {
        record.core.error = None;
        record.values.error = Some(timeout_value_ref(record, step.version));
    }
    record.core.latest_event = LatestEvent {
        kind: step.kind,
        version: step.version,
        fence: record.core.fence(),
    };
    record.ordering = step.ordering;
    record.ttl_ms = step.ttl_ms;
    record.terminal_retention_until = step.retention_until;
    if step.state != b"running" {
        record.governance = None;
    }
    let previous = step.previous.map_or(0, |i| trace.steps[i].group_checkpoint);
    for patch in &trace.owners[step.owner].group_updates[previous..step.group_checkpoint] {
        let patch = &trace.groups[*patch];
        let group = record
            .child_groups
            .get_mut(patch.id)
            .ok_or(ApplyError::InvalidAuthority)?;
        group.summary = patch.summary;
        group.resolved = patch.resolved;
        for (id, status, result) in &patch.updates {
            let member = group
                .children
                .get_mut(*id)
                .ok_or(ApplyError::InvalidAuthority)?;
            member.status = *status;
            member.result = result.map(|(status, result_ref, error_ref)| {
                let [result_ref, error_ref] =
                    [result_ref, error_ref].map(|r| r.map(|r| own_trace_reference(trace, r)));
                ChildResult {
                    status,
                    result_ref,
                    error_ref,
                }
            });
        }
    }
    Ok(())
}

fn own_trace_reference(
    trace: &TimeoutTrace<'_>,
    reference: TraceValueRef<'_>,
) -> super::ActiveValueRef {
    match reference {
        TraceValueRef::Original(reference) => reference.clone(),
        TraceValueRef::Timeout { owner, version } => {
            timeout_value_ref(trace.owners[owner].original, version)
        }
    }
}
