//! Single-Flow lifecycle mutations on the owning shard (ADR-014 step 3).
//!
//! The domain reducer decides the lifecycle record; this module applies the
//! fields beyond it as Elixir does: attribute and state metadata updates,
//! named values, the version of each value reference (the record version
//! that wrote it), priority, and the removal time of a Flow that became
//! terminal (the shard's clock plus its retention, not the client's clock).

use wf_application::commands::flow::history::HistoryEvent;
use wf_application::commands::flow::mutate::{Change, Mutation};
use wf_application::commands::flow::record::Include;
use wf_application::commands::flow::{Term, term};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::{
    self, Cancel, CommittedCommand, Operation, Outcome, StepContinue, Terminal, Transition,
};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use super::flow::Executed;
use super::{Key, ShardState, flow_key};

mod meta;

use meta::updated_meta;

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
            policy,
        } => Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(mutation)?,
            run_at: at(*run_at_ms),
            policy_override: policy.clone(),
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
        Change::ExtendLease { lease_ms, .. } => Operation::ExtendLease(ExtendLease {
            guard: guard(mutation)?,
            lease_ms: *lease_ms,
        }),
    })
}

impl ShardState {
    /// Decides one mutation; `wall` is the shard's committed time.
    pub(crate) fn mutate(
        &mut self,
        wall: u64,
        mutation: &Mutation,
        partition: Partition,
    ) -> Executed {
        // A completion or failure past the active-time limit fails the Flow
        // for it instead, as Elixir does (audit gap G21).
        let now = mutation.now_ms.unwrap_or(wall);
        if matches!(
            mutation.change,
            Change::Complete { .. } | Change::Fail { .. }
        ) && self.any_overdue(now)
        {
            let key: Key = flow_key(partition.clone(), mutation.id.clone());
            if self.warm(&key).is_ok() && self.overdue(&key, now) {
                let logged = self.time_out_one(wall, now, &key);
                let exceeded = ServiceError::rejected("ERR flow max_active_ms exceeded".to_owned());
                return (Err(exceeded), logged);
            }
        }
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
        let key: Key = flow_key(partition, mutation.id.clone());
        self.warm(&key)?;
        let defaults = self.defaults();
        let entry = self
            .hot(&key)
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
                let meta = updated_meta(
                    (mutation, &key.0),
                    current,
                    (&entry.record, &write.record),
                    wall,
                )?;
                let priority = match &mutation.change {
                    Change::Transition {
                        priority: Some(priority),
                        ..
                    } => *priority,
                    _ => entry.priority,
                };
                // Elixir's FIFO entry check: a transition or step must name
                // the partition; a retry need not.
                let explicit = match &mutation.change {
                    Change::Transition { .. } | Change::StepContinue { .. } => {
                        Some(mutation.partition_key.is_some())
                    }
                    _ => None,
                };
                if matches!(
                    mutation.change,
                    Change::Transition { .. } | Change::StepContinue { .. } | Change::Retry { .. }
                ) {
                    self.fifo_entry(&write.record, priority, explicit)?;
                }
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
            Change::StepContinue { .. }
            | Change::ExtendLease {
                ok_on_success: false,
                ..
            } => record(),
            _ => term::Term::text("OK"),
        }
    }
}
