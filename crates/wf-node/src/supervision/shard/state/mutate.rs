//! Single-Flow lifecycle mutations on the owning shard (ADR-014 step 3).
//!
//! The domain reducer decides the lifecycle record; this module applies the
//! fields beyond it as Elixir does: attribute and state metadata updates,
//! named values, the version of each value reference (the record version
//! that wrote it), priority, and the removal time of a Flow that became
//! terminal (the shard's clock plus its retention, not the client's clock).

use wf_application::commands::flow::history::HistoryEvent;
use wf_application::commands::flow::meta;
use wf_application::commands::flow::mutate::{Change, Mutation};
use wf_application::commands::flow::record::{FlowMeta, Include};
use wf_application::commands::flow::{Term, term};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::RetryOverride;
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::{
    self, Cancel, CommittedCommand, Operation, Outcome, StepContinue, Terminal, Transition,
    WorkflowRecord, WorkflowStatus,
};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use super::flow::Executed;
use super::{Key, ShardState};

fn token(bytes: Option<&[u8]>) -> Result<Option<LeaseToken>, ServiceError> {
    bytes
        .map(|bytes| LeaseToken::new(bytes.to_vec()).map_err(ServiceError::Domain))
        .transpose()
}

fn guard(mutation: &Mutation) -> Result<LeaseGuard, ServiceError> {
    Ok(LeaseGuard {
        token: token(mutation.lease_token.as_deref())?
            .ok_or_else(|| ServiceError::flow("lease_token", "must be a non-empty string"))?,
        fence: Fence::new(mutation.fence),
    })
}

fn at(ms: Option<u64>) -> Option<TimestampMillis> {
    ms.map(TimestampMillis::new)
}

/// The domain operation of a mutation.
fn operation(mutation: &Mutation) -> Result<Operation, ServiceError> {
    Ok(match &mutation.change {
        Change::Transition {
            from,
            to,
            run_at_ms,
            payload,
            ..
        } => Operation::Transition(Transition {
            from: from.clone(),
            to: to.clone(),
            fence: Fence::new(mutation.fence),
            lease_token: token(mutation.lease_token.as_deref())?,
            run_at: at(*run_at_ms),
            payload: payload.clone(),
        }),
        Change::StepContinue {
            from,
            to,
            lease_ms,
            worker,
            payload,
            ..
        } => Operation::StepContinue(StepContinue {
            guard: guard(mutation)?,
            from: from.clone(),
            to: to.clone(),
            worker: worker.clone(),
            lease_ms: *lease_ms,
            payload: payload.clone(),
        }),
        // Elixir always resolves the retry policy: an explicit time replaces
        // the computed one, and the retry limit still applies.
        Change::Retry {
            run_at_ms,
            payload,
            error,
        } => Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(mutation)?,
            run_at: at(*run_at_ms),
            policy_override: RetryOverride::default(),
            jitter: JitterSample::new(0),
            payload: payload.clone(),
            error: error.clone(),
        }),
        Change::Complete {
            payload, result, ..
        } => Operation::Complete(Terminal {
            guard: guard(mutation)?,
            payload: payload.clone(),
            value: result.clone(),
        }),
        Change::Fail { payload, error, .. } => Operation::Fail(Terminal {
            guard: guard(mutation)?,
            payload: payload.clone(),
            value: error.clone(),
        }),
        Change::Cancel {
            expected_version,
            expect_state,
            reason,
            ..
        } => Operation::Cancel(Cancel {
            fence: Fence::new(mutation.fence),
            lease_token: token(mutation.lease_token.as_deref())?,
            expected_state: expect_state.clone(),
            expected_version: *expected_version,
            error: reason.clone(),
        }),
        Change::ExtendLease { lease_ms } => Operation::ExtendLease(ExtendLease {
            guard: guard(mutation)?,
            lease_ms: *lease_ms,
        }),
    })
}

/// The state whose metadata a mutation updates.
fn meta_state<'a>(mutation: &'a Mutation, record: &'a WorkflowRecord) -> &'a [u8] {
    match &mutation.change {
        Change::Transition { to, .. } | Change::StepContinue { to, .. } => to,
        Change::Complete { .. } => b"completed",
        Change::Fail { .. } => b"failed",
        Change::Cancel { .. } => b"cancelled",
        Change::Retry { .. } | Change::ExtendLease { .. } => {
            record.run_state().unwrap_or(record.state())
        }
    }
}

/// The fields beyond the record after an accepted mutation.
fn updated_meta(
    mutation: &Mutation,
    current: &FlowMeta,
    record: &WorkflowRecord,
    wall: u64,
) -> Result<FlowMeta, ServiceError> {
    let mut next = current.clone();
    let version = record.version();
    next.attributes = meta::update_attributes(
        &current.attributes,
        &mutation.attributes_merge,
        &mutation.attributes_delete,
    )?;
    next.state_meta = meta::merge_state_meta(
        &current.state_meta,
        meta_state(mutation, record),
        &mutation.state_meta,
    )?;
    super::value::write_values(&mut next, &mutation.values, &mutation.override_values)?;
    for name in &mutation.drop_values {
        next.values.remove(name);
    }
    let (payload, result, error, ttl) = match &mutation.change {
        Change::Transition { payload, .. } | Change::StepContinue { payload, .. } => {
            (payload.is_some(), false, false, None)
        }
        Change::Retry { payload, error, .. } => (payload.is_some(), false, error.is_some(), None),
        Change::Complete {
            payload,
            result,
            ttl_ms,
        } => (payload.is_some(), result.is_some(), false, *ttl_ms),
        Change::Fail {
            payload,
            error,
            ttl_ms,
        } => (payload.is_some(), false, error.is_some(), *ttl_ms),
        Change::Cancel { reason, ttl_ms, .. } => (false, false, reason.is_some(), *ttl_ms),
        Change::ExtendLease { .. } => (false, false, false, None),
    };
    if payload {
        next.payload_version = version;
    }
    if result {
        next.result_version = version;
    }
    if error {
        next.error_version = version;
    }
    if matches!(
        record.status(),
        WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled
    ) {
        next.ttl_ms = ttl;
        next.terminal_until = Some(wall.saturating_add(ttl.unwrap_or(current.retention_ttl_ms)));
    }
    Ok(next)
}

impl ShardState {
    /// Decides one mutation; `wall` is the shard's committed time.
    pub(crate) fn mutate(
        &mut self,
        wall: u64,
        mutation: &Mutation,
        partition: Partition,
    ) -> Executed {
        match self.try_mutate(wall, mutation, partition) {
            Ok((reply, logged)) => (Ok(reply), logged),
            Err(error) => (Err(error), None),
        }
    }

    pub(super) fn try_mutate(
        &mut self,
        wall: u64,
        mutation: &Mutation,
        partition: Partition,
    ) -> Result<(Term, Option<super::Logged>), ServiceError> {
        let now = mutation.now_ms.unwrap_or(wall);
        let key: Key = (partition, mutation.id.clone());
        let defaults = self.defaults();
        let entry = self
            .records
            .get(&key)
            .ok_or_else(|| ServiceError::rejected("ERR flow not found".to_owned()))?;
        let command = CommittedCommand {
            id: RecordId::new(mutation.id.clone()).map_err(|_| ServiceError::InvalidId)?,
            now: TimestampMillis::new(now),
            operation: operation(mutation)?,
        };
        let (outcome, write, history) = workflow::reduce_public(
            Some(&entry.record),
            &command,
            self.policy(entry.record.workflow_type())
                .map(|installed| &installed.retry),
        )
        .map_err(ServiceError::Domain)?
        .into_parts();
        let logged = match (outcome, write) {
            (Outcome::Changed, Some(write)) => {
                let current = entry.meta.as_deref().unwrap_or(&defaults);
                let meta = updated_meta(mutation, current, &write.record, wall)?;
                let priority = match &mutation.change {
                    Change::Transition {
                        priority: Some(priority),
                        ..
                    } => *priority,
                    _ => entry.priority,
                };
                let mut event = HistoryEvent::of(&write.record, priority, &meta, &key.0);
                event.retry = history.and_then(|entry| entry.retry);
                self.put(
                    key.clone(),
                    (write.record, priority, Some(Box::new(meta))),
                    Some(&event),
                );
                Some(self.after_image(&key)?)
            }
            _ => None,
        };
        Ok((self.mutation_reply(&key, mutation), logged))
    }

    fn mutation_reply(&self, key: &Key, mutation: &Mutation) -> Term {
        let record = || {
            self.record_term(key, &Include::default())
                .unwrap_or(Term::Nil)
        };
        match &mutation.change {
            Change::StepContinue { compact: true, .. } => {
                let record = record();
                let get = |name: &str| record.get(name.as_bytes()).cloned().unwrap_or(Term::Nil);
                Term::List(vec![
                    get("id"),
                    get("partition_key"),
                    get("lease_token"),
                    get("fencing_token"),
                ])
            }
            Change::StepContinue { .. } | Change::ExtendLease { .. } => record(),
            _ => term::Term::text("OK"),
        }
    }
}

/// A batch's items on this shard: request position, mutation, partition.
pub(crate) type BatchItems = Vec<(usize, Mutation, Partition)>;
/// Per-item replies by request position.
pub(crate) type BatchReplies = Vec<(usize, Result<Term, ServiceError>)>;

impl ShardState {
    /// Decides a batch's items on this shard. An atomic batch changes
    /// nothing unless every item succeeds: after the first failure the
    /// earlier items are put back and their changes are not logged.
    pub(crate) fn mutate_batch(
        &mut self,
        wall: u64,
        items: BatchItems,
        atomic: bool,
    ) -> (BatchReplies, Vec<super::Logged>) {
        let mut replies = Vec::with_capacity(items.len());
        let mut logged = Vec::new();
        let mut saved: Vec<(Key, Option<Saved>)> = Vec::new();
        for (position, mutation, partition) in items {
            let key: Key = (partition.clone(), mutation.id.clone());
            if atomic {
                saved.push((key.clone(), self.save(&key)));
            }
            match self.try_mutate(wall, &mutation, partition) {
                Ok((reply, change)) => {
                    logged.extend(change);
                    replies.push((position, Ok(reply)));
                }
                Err(error) if atomic => {
                    self.roll_back(saved);
                    let mut failed: BatchReplies = replies
                        .into_iter()
                        .map(|(earlier, _)| (earlier, Err(error.clone())))
                        .collect();
                    failed.push((position, Err(error)));
                    return (failed, Vec::new());
                }
                Err(error) => replies.push((position, Err(error))),
            }
        }
        (replies, logged)
    }

    /// Puts back the Flows an atomic batch changed, latest first.
    fn roll_back(&mut self, saved: Vec<(Key, Option<Saved>)>) {
        for (key, entry) in saved.into_iter().rev() {
            self.restore_saved(key, entry);
        }
    }

    fn save(&self, key: &Key) -> Option<Saved> {
        self.records.get(key).map(|entry| Saved {
            record: entry.record.clone(),
            priority: entry.priority,
            meta: entry.meta.clone(),
            history: entry.history.clone(),
        })
    }

    fn restore_saved(&mut self, key: Key, saved: Option<Saved>) {
        match saved {
            Some(saved) => {
                self.reinstate(
                    key,
                    (saved.record, saved.priority, saved.meta, saved.history),
                );
            }
            None => {
                self.take_entry(&key);
            }
        }
    }
}

/// A Flow as it was before an atomic batch touched it.
struct Saved {
    record: WorkflowRecord,
    priority: u8,
    meta: Option<Box<FlowMeta>>,
    history: super::events::History,
}
