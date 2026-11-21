//! `FLOW.SIGNAL` on the owning shard (ADR-014 step 7), as Elixir's
//! `do_flow_signal` applies it: the record's version rises; with
//! `transition_to` the Flow moves to that state (allowed only from an
//! `if_state` and never from a terminal state), its lease is released and it
//! falls due at `run_at_ms` or now, keeping its run state. Named values are
//! written and a `signaled` event names the signal. An idempotency key
//! remembers the content it applied: the same content again changes nothing
//! (Elixir from feature level 5), different content is refused.

use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_application::commands::flow::signal::Signal;
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::history::EventKind;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_types::version::TimestampMillis;

use super::super::command::Logged;
use super::flow::Executed;
use super::value::write_values;
use super::{Key, ShardState, flow_key};

fn rejected(text: &str) -> ServiceError {
    ServiceError::rejected(text.to_owned())
}

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

/// The record after the signal.
fn signaled(record: &WorkflowRecord, signal: &Signal, now: u64) -> Option<WorkflowRecord> {
    let (latest_kind, _, _) = record.latest_event_witness();
    let version = record.version().checked_add(1)?;
    let mut data = WorkflowRecordData {
        id: record.id().clone(),
        workflow_type: record.workflow_type().to_vec(),
        state: record.state().to_vec(),
        version,
        fence: record.fence(),
        attempts: record.attempts(),
        created_at: record.created_at(),
        updated_at: TimestampMillis::new(now),
        next_run_at: record.next_run_at(),
        run_state: record.run_state().map(<[u8]>::to_vec),
        lease: record.lease().cloned(),
        payload: record.payload().map(<[u8]>::to_vec),
        result: record.result().map(<[u8]>::to_vec),
        error: record.error().map(<[u8]>::to_vec),
        // The domain has no `signaled` witness: a retry witness must not
        // survive it, and a move is a transition.
        latest_kind: match latest_kind {
            EventKind::Retry => EventKind::LeaseExtended,
            kind => kind,
        },
        latest_version: version,
        latest_fence: record.fence(),
    };
    if let Some(state) = &signal.transition_to {
        data.state.clone_from(state);
        data.next_run_at = Some(TimestampMillis::new(signal.run_at_ms.unwrap_or(now)));
        data.lease = None;
        data.latest_kind = EventKind::Transitioned;
    }
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    restore_record(data, limits).ok()
}

impl ShardState {
    /// `FLOW.SIGNAL` on the shard owning its Flow.
    pub(crate) fn signal(&mut self, wall: u64, signal: &Signal, partition: Partition) -> Executed {
        match self.try_signal(wall, signal, partition) {
            Ok(logged) => (Ok(Term::text("OK")), logged),
            Err(error) => (Err(error), None),
        }
    }

    fn try_signal(
        &mut self,
        wall: u64,
        signal: &Signal,
        partition: Partition,
    ) -> Result<Option<Logged>, ServiceError> {
        let now = signal.now_ms.unwrap_or(wall);
        let key: Key = flow_key(partition, signal.id.clone());
        let defaults = self.defaults();
        let entry = self
            .hot(&key)
            .ok_or_else(|| rejected("ERR flow not found"))?;
        let mut meta = entry.meta.as_deref().cloned().unwrap_or(defaults);
        let digest = signal.digest();
        if let Some(key) = &signal.idempotency_key {
            match meta.signal_keys.get(key) {
                Some(stored) if *stored == digest => return Ok(None),
                Some(_) => return Err(rejected("ERR flow signal idempotency conflict")),
                None => {}
            }
        }
        if let Some(target) = &signal.transition_to {
            if terminal(entry.record.state()) {
                return Err(rejected("ERR flow is terminal; use FLOW.REWIND"));
            }
            if signal.if_state.is_empty() {
                return Err(rejected("ERR flow signal transition requires if_state"));
            }
            if !signal
                .if_state
                .iter()
                .any(|state| state.as_slice() == entry.record.state())
            {
                return Err(rejected("ERR flow state mismatch"));
            }
            if terminal(target) {
                return Err(rejected(
                    "ERR terminal flow state requires FLOW.COMPLETE, FLOW.FAIL, or FLOW.CANCEL",
                ));
            }
        }
        let priority = entry.priority;
        let record = signaled(&entry.record, signal, now).ok_or(ServiceError::Unavailable)?;
        // A move into a FIFO state needs a named partition, checked before
        // any value is written. A passive signal enters no lane, so it is
        // not checked (Elixir also refuses one on a claimed FIFO Flow).
        if signal.transition_to.is_some() {
            self.fifo_entry(&record, priority, Some(signal.partition_key.is_some()))?;
        }
        write_values(&mut meta, &signal.values, &signal.override_values)?;
        super::value::drop_values(&mut meta, &signal.drop_values);
        if let Some(key) = &signal.idempotency_key {
            meta.signal_keys.insert(key.clone(), digest);
        }
        let mut event = HistoryEvent::of(&record, priority, &meta, &key.0);
        event.kind = HistoryKind::Signaled;
        event.extras[3] = Some(signal.signal.clone());
        self.put(
            key.clone(),
            (record, priority, Some(Box::new(meta))),
            Some(&event),
        );
        self.after_image(&key).map(Some)
    }
}
