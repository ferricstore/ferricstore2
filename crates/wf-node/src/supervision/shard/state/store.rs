//! Storing a Flow's new state (ADR-014): one place keeps the claim and
//! removal indexes and the history in step with every write.

use wf_application::commands::flow::history::{self, HistoryEvent};
use wf_application::commands::flow::record::FlowMeta;
use wf_domain::workflow::WorkflowRecord;

use super::events::History;
use super::fifo;
use super::{Entry, Key, ShardState};

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

impl ShardState {
    /// Stores a reducer write, keeping the record's priority and fields.
    pub(super) fn store(&mut self, key: Key, record: WorkflowRecord, new_priority: Option<u8>) {
        let mut previous = self.take_entry(&key);
        let priority = previous
            .as_ref()
            .map_or(new_priority.unwrap_or(0), |previous| previous.priority);
        // Moved, not copied: `write` reads only the previous record and
        // history.
        let meta = previous.as_mut().and_then(|previous| previous.meta.take());
        self.write(key, (record, priority, meta), previous, &[]);
    }

    /// Stores a whole Flow, replacing any record under its key; `event` is
    /// the change's own history event when it carries more than the record
    /// shows (a retry's audit).
    pub(super) fn put(
        &mut self,
        key: Key,
        flow: (WorkflowRecord, u8, Option<Box<FlowMeta>>),
        event: Option<&HistoryEvent>,
    ) {
        let flow = self.stamped(flow);
        let previous = self.take_entry(&key);
        self.write(
            key,
            flow,
            previous,
            event.map(std::slice::from_ref).unwrap_or_default(),
        );
    }

    /// Stores a whole Flow with the events its change appended, in order;
    /// a change that went through several versions at once (a step chain,
    /// or its replay) appends each.
    pub(super) fn put_events(
        &mut self,
        key: Key,
        flow: (WorkflowRecord, u8, Option<Box<FlowMeta>>),
        events: &[HistoryEvent],
    ) {
        let flow = self.stamped(flow);
        let previous = self.take_entry(&key);
        self.write(key, flow, previous, events);
    }

    /// A live whole-Flow write records the attribute names and state
    /// metadata key its type's policy indexes now, as Elixir refreshes them
    /// on each write (ADR-014 step 11); replay keeps the logged ones.
    /// Reducer writes (`store`) keep the names the Flow had, since their
    /// replay could not see the policy of the moment.
    fn stamped(
        &self,
        (record, priority, mut meta): (WorkflowRecord, u8, Option<Box<FlowMeta>>),
    ) -> (WorkflowRecord, u8, Option<Box<FlowMeta>>) {
        if let (true, Some(meta)) = (self.live, meta.as_mut()) {
            let policy = self
                .policy(record.workflow_type())
                .map(|installed| &installed.policy);
            meta.indexed_attributes = policy
                .and_then(|policy| policy.indexed_attributes.clone())
                .unwrap_or_default();
            meta.indexed_state_meta = policy.and_then(|policy| policy.indexed_state_meta.clone());
        }
        (record, priority, meta)
    }

    /// Inserts the new state of a Flow; a new version appends its history
    /// event to the history the Flow had.
    fn write(
        &mut self,
        key: Key,
        (record, priority, meta): (WorkflowRecord, u8, Option<Box<FlowMeta>>),
        previous: Option<Box<Entry>>,
        appended: &[HistoryEvent],
    ) {
        let was_terminal = previous
            .as_ref()
            .is_some_and(|previous| terminal(previous.record.state()));
        let was_running = previous
            .as_ref()
            .is_some_and(|previous| previous.record.state() == b"running");
        self.note_credit(was_running, &record, meta.as_deref());
        if !was_terminal && terminal(record.state()) {
            self.note_child(&key, &record, meta.as_deref());
            self.note_closed(&key, meta.as_deref());
        }
        // A Flow that enters another logical state takes a new place in
        // its FIFO lane, as Elixir stamps `state_enter_seq`.
        let (sequence, mut events, before, slot) = match previous {
            Some(mut previous) => {
                let entered = fifo::logical_state(&previous.record) != fifo::logical_state(&record)
                    && !terminal(record.state());
                let sequence = if entered {
                    self.sequence()
                } else {
                    previous.sequence
                };
                let history = std::mem::take(&mut previous.history);
                (
                    sequence,
                    history,
                    Some(previous.record.version()),
                    Some(previous),
                )
            }
            None => (self.sequence(), History::default(), None, None),
        };
        if before.is_none_or(|version| record.version() > version) {
            let defaults = FlowMeta {
                history_max_events: history::default_cap(),
                ..FlowMeta::default()
            };
            let flow_meta = meta.as_deref().unwrap_or(&defaults);
            if appended.is_empty() {
                let event = HistoryEvent::of(&record, priority, flow_meta, &key.0);
                events.push(&event, flow_meta.history_max_events);
            }
            for event in appended {
                events.push(event, flow_meta.history_max_events);
            }
        }
        if let Some(meta) = meta.as_deref()
            && !meta.value_refs.is_empty()
        {
            self.hold(&key, meta.value_refs.values());
        }
        let entry = Entry {
            record,
            priority,
            sequence,
            meta,
            history: events,
        };
        // The Flow keeps its box across changes.
        let boxed = match slot {
            Some(mut boxed) => {
                *boxed = entry;
                boxed
            }
            None => Box::new(entry),
        };
        self.insert(key, boxed);
    }

    /// Puts a Flow back exactly as it was, history included.
    pub(super) fn reinstate(
        &mut self,
        key: Key,
        (record, priority, meta, events): (WorkflowRecord, u8, Option<Box<FlowMeta>>, History),
    ) {
        let sequence = match self.take_entry(&key) {
            Some(previous) => previous.sequence,
            None => self.sequence(),
        };
        self.insert(
            key,
            Box::new(Entry {
                record,
                priority,
                sequence,
                meta,
                history: events,
            }),
        );
    }

    pub(super) fn sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }

    /// Removes a record and its index entries.
    pub(super) fn take_entry(&mut self, key: &Key) -> Option<Box<Entry>> {
        let previous = self.take_slot(key)?;
        self.unindex(key, &previous);
        self.unschedule_removal(&previous);
        self.unschedule_deadline(&previous);
        Some(previous)
    }

    /// [`Self::take_entry`] by a lookup pair.
    pub(super) fn take_entry_parts(&mut self, key: &super::Parts) -> Option<Box<Entry>> {
        let stored = self
            .records
            .get_key_value(key)
            .map(|(stored, _)| stored.clone())?;
        self.take_entry(&stored)
    }

    fn insert(&mut self, key: Key, entry: Box<Entry>) {
        self.index(&key, &entry);
        self.schedule_removal(&key, &entry);
        self.schedule_deadline(&key, &entry);
        if terminal(entry.record.state()) {
            self.cold.demote.push(key.clone());
        }
        self.records.insert(key, super::cold::Slot::Hot(entry));
    }
}
