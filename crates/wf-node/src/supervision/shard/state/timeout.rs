//! Active-time limits and `FLOW.RETENTION_CLEANUP` (ADR-014 step 9).
//!
//! A live Flow with `max_active_ms` must finish by its creation time plus
//! that limit. Past it, a timeout fails the Flow as Elixir's
//! `flow_active_timeout_record/4` does: state `failed`, lease and due time
//! cleared, run state kept, the error the term `{reason, max_active_ms}`, a
//! `failed` event naming both, and the terminal retention started. The
//! shard times Flows out as Elixir's sweeper does (first ten minutes after
//! start, then every ten minutes), and on `FLOW.RETENTION_CLEANUP`, which
//! also removes terminal Flows whose retention ended, both counted against
//! one limit.

use wf_application::commands::flow::cleanup::Cleaned;
use wf_application::commands::flow::create::MaxActive;
use wf_application::commands::flow::history::HistoryEvent;
use wf_application::commands::flow::term::{Term, map};
use wf_domain::history::EventKind;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_types::version::TimestampMillis;

use super::{Entry, Key, Logged, ShardState};

/// Elixir's `flow_retention_sweeper_interval_ms` and initial delay.
const SWEEP_EVERY_MS: u64 = 600_000;

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

impl ShardState {
    /// The deadline slot of a live Flow with an active-time limit.
    fn deadline_slot(entry: &Entry) -> Option<(u64, u64)> {
        let Some(MaxActive::Ms(limit)) = entry.meta.as_ref()?.max_active_ms else {
            return None;
        };
        (!terminal(entry.record.state())).then(|| {
            (
                entry.record.created_at().get().saturating_add(limit),
                entry.sequence,
            )
        })
    }

    pub(super) fn schedule_deadline(&mut self, key: &Key, entry: &Entry) {
        if let Some(slot) = Self::deadline_slot(entry) {
            self.deadlines.insert(slot, key.clone());
        }
    }

    pub(super) fn unschedule_deadline(&mut self, entry: &Entry) {
        if let Some(slot) = Self::deadline_slot(entry) {
            self.deadlines.remove(&slot);
        }
    }

    /// The periodic timeout sweep, at Elixir's sweeper interval.
    pub(crate) fn sweep_timeouts(&mut self, now: u64, max: usize) -> Vec<Logged> {
        if self.timeouts_due == 0 {
            self.timeouts_due = now.saturating_add(SWEEP_EVERY_MS);
        }
        if now < self.timeouts_due {
            return Vec::new();
        }
        let logged = self.time_out(now, now, max);
        if logged.len() < max {
            self.timeouts_due = now.saturating_add(SWEEP_EVERY_MS);
        }
        logged
    }

    /// Fails up to `max` Flows whose active time ended by `now`.
    fn time_out(&mut self, wall: u64, now: u64, max: usize) -> Vec<Logged> {
        let keys: Vec<Key> = self
            .deadlines
            .range(..=(now, u64::MAX))
            .take(max)
            .map(|(_, key)| key.clone())
            .collect();
        keys.iter()
            .filter_map(|key| self.time_out_one(wall, now, key))
            .collect()
    }

    fn time_out_one(&mut self, wall: u64, now: u64, key: &Key) -> Option<Logged> {
        let entry = self.records.get(key)?;
        let mut meta = entry.meta.as_deref()?.clone();
        let Some(MaxActive::Ms(limit)) = meta.max_active_ms else {
            return None;
        };
        let record = &entry.record;
        let version = record.version().checked_add(1)?;
        let mut error = Vec::new();
        map(vec![
            ("max_active_ms", Term::int(limit)),
            ("reason", Term::text("max_active_ms")),
        ])
        .encode(&mut error)?;
        let data = WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: b"failed".to_vec(),
            version,
            fence: record.fence(),
            attempts: record.attempts(),
            created_at: record.created_at(),
            updated_at: TimestampMillis::new(now.max(record.updated_at().get())),
            next_run_at: None,
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: None,
            payload: record.payload().map(<[u8]>::to_vec),
            result: record.result().map(<[u8]>::to_vec),
            error: Some(error),
            latest_kind: EventKind::Failed,
            latest_version: version,
            latest_fence: record.fence(),
        };
        let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
        let failed = restore_record(data, limits).ok()?;
        meta.error_version = version;
        meta.error_term = true;
        meta.ttl_ms = None;
        meta.terminal_until = Some(now.max(wall).saturating_add(meta.retention_ttl_ms));
        let priority = entry.priority;
        let mut event = HistoryEvent::of(&failed, priority, &meta, &key.0);
        event.extras[4] = Some(b"max_active_ms".to_vec());
        event.extras[5] = Some(limit.to_string().into_bytes());
        self.put(
            key.clone(),
            (failed, priority, Some(Box::new(meta))),
            Some(&event),
        );
        self.after_image(key).ok()
    }

    /// `FLOW.RETENTION_CLEANUP` on this shard: timeouts first, then
    /// removals, up to `limit` together.
    pub(crate) fn cleanup(&mut self, wall: u64, now: u64, limit: u64) -> (Cleaned, Vec<Logged>) {
        let max = usize::try_from(limit).unwrap_or(usize::MAX);
        let mut logged = self.time_out(wall, now, max);
        let timeouts = logged.len();
        let remaining = max.saturating_sub(timeouts);
        let removable: Vec<&Entry> = self
            .expiries
            .range(..=(now, u64::MAX))
            .take(remaining)
            .filter_map(|(_, key)| self.records.get(key).map(|entry| &**entry))
            .collect();
        let flows = removable.len();
        let history: u64 = removable
            .iter()
            .map(|entry| u64::from(entry.history.len()))
            .sum();
        let values: usize = removable
            .iter()
            .map(|entry| {
                let record = &entry.record;
                let named = entry.meta.as_ref().map_or(0, |meta| meta.values.len());
                [record.payload(), record.result(), record.error()]
                    .iter()
                    .flatten()
                    .count()
                    .saturating_add(named)
            })
            .sum();
        logged.extend(self.sweep(now, remaining));
        let count = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
        let cleaned = Cleaned {
            flows: count(flows),
            history,
            values: count(values),
            active_timeouts: count(timeouts),
        };
        (cleaned, logged)
    }
}
