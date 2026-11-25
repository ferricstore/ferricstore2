//! Start-and-claim and deterministic step chains on the owning shard
//! (ADR-014 step 6).
//!
//! A step chain decides every version of one Flow with the domain reducer
//! (start claimed, continue through each state one millisecond apart,
//! complete) and stores the result as one change whose after-image carries
//! all of its events. Each continuation keeps the first lease deadline, as
//! Elixir's chain does.

use wf_application::commands::flow::history::HistoryEvent;
use wf_application::commands::flow::record::{FlowMeta, Include};
use wf_application::commands::flow::start::{RunSteps, StartAndClaim};
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::lease::LeaseGuard;
use wf_domain::workflow::{
    self, CommittedCommand, Operation, StartAndClaim as Start, StepContinue, Terminal,
    WorkflowRecord,
};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

use super::super::command::Logged;
use super::flow::Executed;
use super::{Key, ShardState, flow_key};

fn decide(
    record: Option<&WorkflowRecord>,
    id: &[u8],
    now: u64,
    operation: Operation,
) -> Result<WorkflowRecord, ServiceError> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec()).map_err(|_| ServiceError::InvalidId)?,
        now: TimestampMillis::new(now),
        operation,
    };
    let (_, write, _) = workflow::reduce_public(record, &command, None)
        .map_err(ServiceError::Domain)?
        .into_parts();
    write
        .map(|write| write.record)
        .ok_or(ServiceError::Unavailable)
}

fn guard(record: &WorkflowRecord) -> Result<LeaseGuard, ServiceError> {
    let lease = record.lease().ok_or(ServiceError::Unavailable)?;
    Ok(LeaseGuard {
        token: lease.token.clone(),
        fence: record.fence(),
    })
}

/// Elixir's start-and-claim reply: the record map, which always names
/// `max_active_ms` and `value_refs`.
fn start_reply(record: Term) -> Term {
    let Term::Map(mut entries) = record else {
        return record;
    };
    for key in [&b"max_active_ms"[..], b"value_refs"] {
        if !entries.iter().any(|(name, _)| name.as_slice() == key) {
            let value = if key == b"value_refs" {
                Term::Map(Vec::new())
            } else {
                Term::Nil
            };
            entries.push((key.to_vec(), value));
        }
    }
    Term::Map(entries)
}

impl ShardState {
    /// `FLOW.START_AND_CLAIM` on the shard owning the new Flow.
    pub(crate) fn start_and_claim(
        &mut self,
        wall: u64,
        start: &StartAndClaim,
        partition: Partition,
    ) -> Executed {
        match self.try_start(wall, start, partition) {
            Ok((reply, logged)) => (Ok(reply), Some(logged)),
            Err(error) => (Err(error), None),
        }
    }

    fn try_start(
        &mut self,
        wall: u64,
        start: &StartAndClaim,
        partition: Partition,
    ) -> Result<(Term, Logged), ServiceError> {
        let create = &start.create;
        let now = create.now_ms.unwrap_or(wall);
        let key: Key = flow_key(partition, create.id.clone());
        if self.records.contains_key(&key) {
            return Err(ServiceError::rejected("ERR flow already exists".to_owned()));
        }
        // A start enters its state running, which a FIFO state refuses as
        // Elixir's `flow_require_fifo_entry/4` does (audit gap G76).
        self.fifo_guard(
            (&create.flow_type, &create.state),
            true,
            create.priority,
            None,
        )?;
        let meta = self.new_meta(create)?;
        let record = decide(
            None,
            &create.id,
            now,
            Operation::StartAndClaim(Start {
                workflow_type: create.flow_type.clone(),
                state: create.state.clone(),
                worker: start.worker.clone(),
                lease_ms: start.lease_ms,
                payload: create.payload.clone(),
            }),
        )?;
        self.put(
            key.clone(),
            (record, create.priority, Some(Box::new(meta))),
            None,
        );
        let logged = self.after_image(&key)?;
        let reply = self
            .record_term(&key, &Include::default())
            .map_or(Term::Nil, start_reply);
        Ok((reply, logged))
    }

    /// `FLOW.RUN_STEPS_MANY` for the items this shard owns, all or
    /// nothing as in Elixir's single state-machine command: when one item
    /// fails, every item here gets that error and none is stored.
    pub(crate) fn run_steps(
        &mut self,
        wall: u64,
        spec: &RunSteps,
        items: Vec<(usize, Vec<u8>, Partition)>,
    ) -> (super::BatchReplies, Vec<Logged>) {
        let mut logged = Vec::new();
        let mut done: Vec<Key> = Vec::new();
        let mut failure = None;
        for (_, id, partition) in &items {
            match self.run_chain(wall, spec, id.clone(), partition.clone()) {
                Ok(change) => {
                    logged.push(change);
                    done.push(flow_key(partition.clone(), id.clone()));
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = failure {
            for key in done {
                self.take_entry(&key);
            }
            let failed = items
                .into_iter()
                .map(|(position, _, _)| (position, Err(error.clone())))
                .collect();
            return (failed, Vec::new());
        }
        let outcomes = items
            .into_iter()
            .map(|(position, _, _)| (position, Ok(Term::text("OK"))))
            .collect();
        (outcomes, logged)
    }

    fn run_chain(
        &mut self,
        wall: u64,
        spec: &RunSteps,
        id: Vec<u8>,
        partition: Partition,
    ) -> Result<Logged, ServiceError> {
        let now = spec.now_ms.unwrap_or(wall);
        let key: Key = flow_key(partition, id);
        if self.records.contains_key(&key) {
            return Err(ServiceError::rejected("ERR flow already exists".to_owned()));
        }
        let first = spec.states.first().ok_or(ServiceError::Unavailable)?;
        let mut meta = FlowMeta {
            retention_ttl_ms: spec.retention_ttl_ms.unwrap_or(self.retention_ms),
            ..self.defaults()
        };
        let mut record = decide(
            None,
            &key.1,
            now,
            Operation::StartAndClaim(Start {
                workflow_type: spec.flow_type.clone(),
                state: first.clone(),
                worker: spec.worker.clone(),
                lease_ms: spec.lease_ms,
                payload: spec.payload.clone(),
            }),
        )?;
        let mut events = vec![HistoryEvent::of(&record, 0, &meta, &key.0)];
        let mut at = now;
        for pair in spec.states.windows(2) {
            let [from, to] = pair else { continue };
            at = at.saturating_add(1);
            let deadline = record.lease().map_or(at, |lease| lease.deadline.get());
            record = decide(
                Some(&record),
                &key.1,
                at,
                Operation::StepContinue(StepContinue {
                    guard: guard(&record)?,
                    from: from.clone(),
                    to: to.clone(),
                    worker: None,
                    lease_ms: deadline.saturating_sub(at).max(1),
                    payload: None,
                }),
            )?;
            events.push(HistoryEvent::of(&record, 0, &meta, &key.0));
        }
        at = at.saturating_add(1);
        record = decide(
            Some(&record),
            &key.1,
            at,
            Operation::Complete(Terminal {
                guard: guard(&record)?,
                payload: None,
                value: spec.result.clone(),
            }),
        )?;
        if spec.result.is_some() {
            meta.result_version = record.version();
        }
        meta.terminal_until = Some(at.max(wall).saturating_add(meta.retention_ttl_ms));
        events.push(HistoryEvent::of(&record, 0, &meta, &key.0));
        let count = events.len();
        self.put_events(key.clone(), (record, 0, Some(Box::new(meta))), &events);
        self.after_image_events(&key, count)
    }
}
