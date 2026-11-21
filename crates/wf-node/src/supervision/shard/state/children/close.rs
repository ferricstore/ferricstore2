//! A closed parent's children (Elixir's
//! `flow_maybe_cancel_children_on_parent_closed`): when a parent becomes
//! terminal, or a group of a parent that children changed is resolved, every
//! group that asks for it (`on_parent_closed: cancel_children`, the default)
//! cancels its running children. The group marks them cancelled (resolving as
//! a failure if it was open), the parent records `children_cancelled`, and
//! each child is cancelled without a lease or due time; a cancelled child
//! closes its own children in turn. The parent and its children change in one
//! logged group, at the parent's time of change.

use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::queue::Partition;
use wf_domain::history::EventKind;
use wf_types::version::TimestampMillis;

use super::super::super::command::Logged;
use super::super::{Key, ShardState, flow_key};
use super::{parent_next, terminal};

impl ShardState {
    /// Queues a live Flow with child groups that became terminal.
    pub(in super::super) fn note_closed(&mut self, key: &Key, meta: Option<&FlowMeta>) {
        if self.live && meta.is_some_and(|meta| !meta.child_groups.is_empty()) {
            self.closing.push(key.clone());
        }
    }

    /// Cancels the running children `parent`'s groups give up, if it closed
    /// or one of those groups is resolved; returns the change to log.
    pub(super) fn close_children(&mut self, wall: u64, parent: &Key) -> Option<Logged> {
        // A parent that closed first may be on disk (ADR-016).
        self.warm(parent).ok()?;
        let entry = self.hot(parent)?;
        let mut meta = entry.meta.as_deref()?.clone();
        let closed = terminal(entry.record.state());
        let due = meta
            .child_groups
            .values()
            .any(|group| group.cancels_children() && (closed || group.resolved.is_some()));
        if !due {
            return None;
        }
        let parent_partition = parent.0.name();
        let mut running = Vec::new();
        for group in meta
            .child_groups
            .values_mut()
            .filter(|group| group.cancels_children())
        {
            running.extend(group.cancel_running(&parent_partition));
        }
        running.sort();
        running.dedup();
        let now = entry.record.updated_at().get();
        let next_run = entry.record.next_run_at().map(TimestampMillis::get);
        let record = parent_next(
            &entry.record,
            entry.record.state(),
            now,
            next_run,
            EventKind::ChildrenCancelled,
        )?;
        let priority = entry.priority;
        let mut history = HistoryEvent::of(&record, priority, &meta, &parent.0);
        history.kind = HistoryKind::Event(EventKind::ChildrenCancelled);
        self.put(
            parent.clone(),
            (record, priority, Some(Box::new(meta))),
            Some(&history),
        );
        let mut group = vec![self.after_image(parent).ok()?];
        for (id, partition) in running {
            let child = flow_key(Partition::parse(&partition), id);
            group.extend(self.cancel_child(wall, &child, now));
        }
        Some(Logged::Group(group))
    }

    /// Cancels one child that is not terminal yet.
    fn cancel_child(&mut self, wall: u64, key: &Key, now: u64) -> Option<Logged> {
        self.warm(key).ok()?;
        let entry = self.hot(key)?;
        if terminal(entry.record.state()) {
            return None;
        }
        let mut meta = entry
            .meta
            .as_deref()
            .cloned()
            .unwrap_or_else(|| self.defaults());
        meta.terminal_until = Some(now.max(wall).saturating_add(meta.retention_ttl_ms));
        let record = parent_next(&entry.record, b"cancelled", now, None, EventKind::Cancelled)?;
        let priority = entry.priority;
        let mut history = HistoryEvent::of(&record, priority, &meta, &key.0);
        history.kind = HistoryKind::Event(EventKind::Cancelled);
        self.put(
            key.clone(),
            (record, priority, Some(Box::new(meta))),
            Some(&history),
        );
        self.after_image(key).ok()
    }
}
