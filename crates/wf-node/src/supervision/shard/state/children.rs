//! Child groups on the shard (ADR-014 step 8), as Elixir's
//! `do_flow_spawn_children` and `flow_maybe_apply_child_terminal` apply them.
//!
//! A spawn checks the parent's guards, creates every child and moves the
//! parent to its wait state without a lease, and logs all of it as one
//! record so the spawn is all or nothing. When a live change makes a Flow
//! with a parent terminal, the write queues a child event; the round settles
//! the queue before it appends: the parent's open group records the child's
//! status and references, and a resolved group moves the parent to its
//! success or failure state and makes it due. Replay re-applies the logged
//! parent changes instead, so the queue only fills once the shard is live.

use wf_application::commands::flow::children::SpawnChildren;
use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_application::commands::flow::keys;
use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::history::EventKind;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{self, CommittedCommand, Create, Operation, WorkflowRecord};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

use super::super::command::Logged;
use super::flow::Executed;
use super::{Key, ShardState, flow_key};

/// A child that became terminal, for its parent's group.
pub(crate) struct ChildEvent {
    pub(crate) parent: Key,
    pub(crate) child: Vec<u8>,
    pub(crate) status: Vec<u8>,
    pub(crate) refs: (Option<Vec<u8>>, Option<Vec<u8>>),
    pub(crate) now: u64,
}

fn rejected(text: &str) -> ServiceError {
    ServiceError::rejected(text.to_owned())
}

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

/// A parent record changed by its children: new state and version, no
/// lease; `due` decides its next run time.
fn parent_next(
    record: &WorkflowRecord,
    state: &[u8],
    now: u64,
    due: Option<u64>,
    kind: EventKind,
) -> Option<WorkflowRecord> {
    let version = record.version().checked_add(1)?;
    let data = WorkflowRecordData {
        id: record.id().clone(),
        workflow_type: record.workflow_type().to_vec(),
        state: state.to_vec(),
        version,
        fence: record.fence(),
        attempts: record.attempts(),
        created_at: record.created_at(),
        updated_at: TimestampMillis::new(now.max(record.updated_at().get())),
        next_run_at: due.map(TimestampMillis::new),
        run_state: record.run_state().map(<[u8]>::to_vec),
        lease: None,
        payload: record.payload().map(<[u8]>::to_vec),
        result: record.result().map(<[u8]>::to_vec),
        error: record.error().map(<[u8]>::to_vec),
        latest_kind: kind,
        latest_version: version,
        latest_fence: record.fence(),
    };
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    restore_record(data, limits).ok()
}

impl ShardState {
    /// `FLOW.SPAWN_CHILDREN` for a parent whose children live on this
    /// shard.
    pub(crate) fn spawn_children(
        &mut self,
        wall: u64,
        spawn: &SpawnChildren,
        partition: Partition,
    ) -> Executed {
        match self.try_spawn(wall, spawn, partition) {
            Ok(logged) => (Ok(Term::text("OK")), logged),
            Err(error) => (Err(error), None),
        }
    }

    fn check_parent(&self, spawn: &SpawnChildren, key: &Key) -> Result<Option<()>, ServiceError> {
        let entry = self
            .records
            .get(key)
            .ok_or_else(|| rejected("ERR flow not found"))?;
        let record = &entry.record;
        if let Some(group) = entry
            .meta
            .as_deref()
            .and_then(|meta| meta.child_groups.get(&spawn.group_id))
        {
            return if group.request_hash == spawn.group.request_hash {
                Ok(None)
            } else {
                Err(rejected("ERR flow child group idempotency conflict"))
            };
        }
        if spawn
            .from_state
            .as_deref()
            .is_some_and(|from| from != record.state())
        {
            return Err(rejected("ERR flow wrong state"));
        }
        let token = record.lease().map(|lease| lease.token.as_bytes());
        if record.fence().get() != spawn.fence || token != spawn.lease_token.as_deref() {
            return Err(rejected("ERR stale flow lease"));
        }
        if terminal(record.state()) {
            return Err(rejected("ERR flow parent is terminal"));
        }
        if spawn.group.wait != wf_application::commands::flow::children::Wait::None
            && spawn.wait_state.is_none()
        {
            return Err(rejected(
                "ERR flow wait_state is required when waiting for children",
            ));
        }
        Ok(Some(()))
    }

    fn try_spawn(
        &mut self,
        wall: u64,
        spawn: &SpawnChildren,
        partition: Partition,
    ) -> Result<Option<Logged>, ServiceError> {
        let now = spawn.now_ms.unwrap_or(wall);
        let key: Key = flow_key(partition, spawn.parent_id.clone());
        if self.check_parent(spawn, &key)?.is_none() {
            return Ok(None);
        }
        let children: Vec<(Key, WorkflowRecord, u8, FlowMeta)> =
            self.prepare_children(spawn, &key, now)?;
        let entry = self.records.get(&key).ok_or(ServiceError::Unavailable)?;
        let mut meta = entry
            .meta
            .as_deref()
            .cloned()
            .unwrap_or_else(|| self.defaults());
        meta.child_groups
            .insert(spawn.group_id.clone(), spawn.group.clone());
        let state = match (&spawn.group.resolved, &spawn.wait_state) {
            (Some(_), _) => spawn.group.success.clone(),
            (None, Some(wait_state)) => wait_state.clone(),
            (None, None) => entry.record.state().to_vec(),
        };
        let priority = entry.priority;
        let record = parent_next(&entry.record, &state, now, None, EventKind::ChildrenSpawned)
            .ok_or(ServiceError::Unavailable)?;
        let mut event = HistoryEvent::of(&record, priority, &meta, &key.0);
        event.kind = HistoryKind::Event(EventKind::ChildrenSpawned);
        self.put(
            key.clone(),
            (record, priority, Some(Box::new(meta))),
            Some(&event),
        );
        let mut group = vec![self.after_image(&key)?];
        for (child_key, record, priority, meta) in children {
            self.put(
                child_key.clone(),
                (record, priority, Some(Box::new(meta))),
                None,
            );
            group.push(self.after_image(&child_key)?);
        }
        Ok(Some(Logged::Group(group)))
    }

    fn prepare_children(
        &self,
        spawn: &SpawnChildren,
        parent: &Key,
        now: u64,
    ) -> Result<Vec<(Key, WorkflowRecord, u8, FlowMeta)>, ServiceError> {
        let entry = self.records.get(parent).ok_or(ServiceError::Unavailable)?;
        let root = entry
            .meta
            .as_deref()
            .and_then(|meta| meta.root_flow_id.clone())
            .unwrap_or_else(|| parent.1.clone());
        let mut prepared = Vec::with_capacity(spawn.children.len());
        for child in &spawn.children {
            let partition = Partition::for_new_flow(&child.id, child.partition_key.as_deref());
            let key: Key = flow_key(partition, child.id.clone());
            if self.records.contains_key(&key) {
                return Err(rejected("ERR flow already exists"));
            }
            self.fifo_guard(
                (child.flow_type.as_slice(), child.state.as_slice()),
                false,
                child.priority,
                Some(child.partition_key.is_some()),
            )?;
            let mut meta = self.new_meta(child)?;
            meta.parent_flow_id = Some(parent.1.clone());
            meta.parent_partition_key = Some(parent.0.name());
            meta.root_flow_id = Some(root.clone()).filter(|root| *root != child.id);
            let command = CommittedCommand {
                id: RecordId::new(child.id.clone()).map_err(|_| ServiceError::InvalidId)?,
                now: TimestampMillis::new(child.now_ms.unwrap_or(now)),
                operation: Operation::Create(Create {
                    workflow_type: child.flow_type.clone(),
                    state: child.state.clone(),
                    run_at: child.run_at_ms.map(TimestampMillis::new),
                    payload: child.payload.clone(),
                }),
            };
            let (_, write, _) = workflow::reduce(None, &command)
                .map_err(ServiceError::Domain)?
                .into_parts();
            let record = write.ok_or(ServiceError::Unavailable)?.record;
            prepared.push((key, record, child.priority, meta));
        }
        Ok(prepared)
    }

    /// Queues the child event of a live change that made a Flow with a
    /// parent terminal.
    pub(super) fn note_child(
        &mut self,
        key: &Key,
        record: &WorkflowRecord,
        meta: Option<&FlowMeta>,
    ) {
        let Some(meta) = meta.filter(|_| self.live) else {
            return;
        };
        let Some(parent_id) = meta.parent_flow_id.clone() else {
            return;
        };
        let parent_partition = meta
            .parent_partition_key
            .as_deref()
            .map_or_else(|| key.0.clone(), Partition::parse);
        let reference = |kind: char, present: bool, version: u64| {
            present.then(|| keys::value_ref(&key.0, kind, &key.1, version))
        };
        self.child_events.push(ChildEvent {
            parent: flow_key(parent_partition, parent_id),
            child: key.1.clone(),
            status: record.state().to_vec(),
            refs: (
                reference('r', record.result().is_some(), meta.result_version),
                reference('e', record.error().is_some(), meta.error_version),
            ),
            now: record.updated_at().get(),
        });
    }

    /// Applies queued child events to parents on this shard, cascading when
    /// a parent itself becomes terminal; returns the changes to log.
    pub(crate) fn settle_children(&mut self, wall: u64) -> Vec<Logged> {
        let mut logged = Vec::new();
        while !self.child_events.is_empty() {
            let events = std::mem::take(&mut self.child_events);
            logged.extend(
                events
                    .iter()
                    .filter_map(|event| self.child_finished(wall, event)),
            );
        }
        logged
    }

    fn child_finished(&mut self, wall: u64, event: &ChildEvent) -> Option<Logged> {
        let entry = self.records.get(&event.parent)?;
        let mut meta = entry.meta.as_deref()?.clone();
        let group = meta
            .child_groups
            .values_mut()
            .find(|group| group.waits_for(&event.child))?;
        group.finish(&event.child, &event.status, event.refs.clone());
        let target = group.target().map(<[u8]>::to_vec);
        let kind = match event.status.as_slice() {
            b"completed" => EventKind::ChildCompleted,
            b"failed" => EventKind::ChildFailed,
            _ => EventKind::ChildCancelled,
        };
        let state = target
            .clone()
            .unwrap_or_else(|| entry.record.state().to_vec());
        let due = match &target {
            Some(state) if !terminal(state) => Some(event.now),
            Some(_) => None,
            None => entry.record.next_run_at().map(TimestampMillis::get),
        };
        let record = parent_next(&entry.record, &state, event.now, due, kind)?;
        if terminal(record.state()) {
            meta.terminal_until = Some(wall.saturating_add(meta.retention_ttl_ms));
        }
        let priority = entry.priority;
        let mut history = HistoryEvent::of(&record, priority, &meta, &event.parent.0);
        history.kind = HistoryKind::Event(kind);
        self.put(
            event.parent.clone(),
            (record, priority, Some(Box::new(meta))),
            Some(&history),
        );
        self.after_image(&event.parent).ok()
    }
}
