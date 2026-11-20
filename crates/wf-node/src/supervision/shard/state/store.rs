//! Storing a Flow's new state (ADR-014): one place keeps the claim and
//! removal indexes and the history in step with every write.

use wf_application::commands::flow::history::{self, HistoryEvent};
use wf_application::commands::flow::record::FlowMeta;
use wf_domain::workflow::WorkflowRecord;

use super::events::History;
use super::{Entry, Key, ShardState};

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

impl ShardState {
    /// Stores a reducer write, keeping the record's priority and fields.
    pub(super) fn store(&mut self, key: Key, record: WorkflowRecord, new_priority: Option<u8>) {
        let previous = self.take_entry(&key);
        let priority = previous
            .as_ref()
            .map_or(new_priority.unwrap_or(0), |previous| previous.priority);
        let meta = previous.as_ref().and_then(|previous| previous.meta.clone());
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
        let previous = self.take_entry(&key);
        self.write(key, flow, previous, events);
    }

    /// Inserts the new state of a Flow; a new version appends its history
    /// event to the history the Flow had.
    fn write(
        &mut self,
        key: Key,
        (record, priority, meta): (WorkflowRecord, u8, Option<Box<FlowMeta>>),
        previous: Option<Entry>,
        appended: &[HistoryEvent],
    ) {
        let was_terminal = previous
            .as_ref()
            .is_some_and(|previous| terminal(previous.record.state()));
        if !was_terminal && terminal(record.state()) {
            self.note_child(&key, &record, meta.as_deref());
        }
        let (sequence, mut events, before) = match previous {
            Some(previous) => (
                previous.sequence,
                previous.history,
                Some(previous.record.version()),
            ),
            None => (self.sequence(), History::default(), None),
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
        self.insert(
            key,
            Entry {
                record,
                priority,
                sequence,
                meta,
                history: events,
            },
        );
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
            Entry {
                record,
                priority,
                sequence,
                meta,
                history: events,
            },
        );
    }

    fn sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }

    /// Removes a record and its index entries.
    pub(super) fn take_entry(&mut self, key: &Key) -> Option<Entry> {
        let previous = self.records.remove(key)?;
        self.unindex(key, &previous);
        self.unschedule_removal(&previous);
        Some(previous)
    }

    fn insert(&mut self, key: Key, entry: Entry) {
        self.index(&key, &entry);
        self.schedule_removal(&key, &entry);
        self.records.insert(key, entry);
    }
}
