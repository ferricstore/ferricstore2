//! Removal of terminal records after their retention (ADR-013).
//!
//! A terminal record is removed `retention_ms` after its last change. The
//! shard sweeps due removals into one logged `Expire` change, so replay
//! removes the same records at the same log position. Terminal records get
//! no other logged change, so nothing else depends on when they go.

use wf_domain::workflow::{Outcome, WorkflowStatus};

use super::{Entry, Key, Logged, Parts, ShardState};

fn terminal(entry: &Entry) -> bool {
    matches!(
        entry.record.status(),
        WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled
    )
}

impl ShardState {
    pub(super) fn removal_slot(&self, entry: &Entry) -> Option<(u64, u64)> {
        if let Some(until) = entry.meta.as_ref().and_then(|meta| meta.terminal_until) {
            return terminal(entry).then_some((until, entry.sequence));
        }
        terminal(entry).then(|| {
            (
                entry.record.updated_at().get().saturating_add(
                    entry
                        .meta
                        .as_ref()
                        .map_or(self.retention_ms, |meta| meta.retention_ttl_ms),
                ),
                entry.sequence,
            )
        })
    }

    pub(super) fn schedule_removal(&mut self, key: &Key, entry: &Entry) {
        if let Some(slot) = self.removal_slot(entry) {
            self.expiries.insert(slot, key.clone());
        }
    }

    pub(super) fn unschedule_removal(&mut self, entry: &Entry) {
        if let Some(slot) = self.removal_slot(entry) {
            self.expiries.remove(&slot);
        }
    }

    /// Removes up to `max` terminal records whose retention ended by `now`;
    /// returns the change to log, if any.
    pub(crate) fn sweep(&mut self, now: u64, max: usize) -> Option<Logged> {
        let keys: Vec<Parts> = self
            .expiries
            .range(..=(now, u64::MAX))
            .take(max)
            .map(|(_, key)| (**key).clone())
            .collect();
        if keys.is_empty() {
            return None;
        }
        (self.expire(&keys) == Outcome::Changed).then_some(Logged::Expire { now, keys })
    }

    /// Removes every named record, or none when one is missing or not
    /// terminal.
    pub(super) fn expire(&mut self, keys: &[Parts]) -> Outcome {
        let removable = !keys.is_empty()
            && keys
                .iter()
                .all(|key| self.records.get(key).is_some_and(|entry| terminal(entry)));
        if !removable {
            return Outcome::NotEligible;
        }
        for key in keys {
            if let Some((stored, entry)) = self.records.remove_entry(key) {
                self.unindex(&stored, &entry);
                self.unschedule_removal(&entry);
                self.unschedule_deadline(&entry);
            }
        }
        Outcome::Changed
    }
}
