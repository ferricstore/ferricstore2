//! Operator changes to a schedule: pause, resume and delete, each held to
//! the caller's `expected_state`/`expected_version` guards; and the wake a
//! finished target gives its queued schedule.

use wf_application::commands::flow::schedule::guard::Expect;
use wf_application::commands::queue::ServiceError;
use wf_domain::history::EventKind;
use wf_types::version::TimestampMillis;

use super::super::super::command::Logged;
use super::super::{Key, ShardState};
use super::{Stored, data_of, not_found, rejected, terminal};

impl ShardState {
    /// Pulls an active schedule's queued occurrence to `now` once the
    /// target it waits on has finished, as Elixir's `wake_queued/4`; any
    /// other schedule, or one already due, is left as it is.
    pub(crate) fn schedule_wake(
        &mut self,
        key: &Key,
        target: &[u8],
        now: u64,
    ) -> Option<(Stored, Logged)> {
        let stored = self.stored(key)?;
        let queued = &stored.definition;
        let waiting = stored.state.as_slice() == b"active"
            && queued.last_target_id.as_deref() == Some(target)
            && queued.overlap_queued_due_at_ms.is_some()
            && queued.next_run_at_ms.is_some_and(|next| next > now);
        if !waiting {
            return None;
        }
        let entry = self.hot(key)?;
        let mut data = data_of(&entry.record);
        let mut definition = stored.definition;
        definition.next_run_at_ms = Some(now);
        data.next_run_at = Some(TimestampMillis::new(now));
        data.payload = Some(definition.encode());
        let meta = self.meta_of(key);
        let logged = self
            .rewrite(key, data, meta, (EventKind::Transitioned, now))
            .ok()?;
        Some((self.stored(key)?, logged))
    }

    /// Pauses an active schedule, or resumes a paused one at its next run.
    pub(crate) fn schedule_pause(
        &mut self,
        key: &Key,
        (pause, expect): (bool, &Expect),
        now: u64,
    ) -> Result<(Stored, Logged), ServiceError> {
        let stored = self.stored(key).ok_or_else(not_found)?;
        expect.check(&stored.state, stored.version)?;
        let entry = self.hot(key).ok_or_else(not_found)?;
        let mut data = data_of(&entry.record);
        if pause {
            if stored.state.as_slice() != b"active" {
                return Err(rejected("ERR flow schedule is not active"));
            }
            data.state = b"paused".to_vec();
        } else {
            if stored.state.as_slice() != b"paused" {
                return Err(rejected("ERR flow schedule is not paused"));
            }
            let next = stored
                .definition
                .next_run_at_ms
                .ok_or_else(|| rejected("ERR flow schedule has no next run time"))?;
            data.state = b"active".to_vec();
            data.next_run_at = Some(TimestampMillis::new(next));
        }
        let meta = self.meta_of(key);
        let logged = self.rewrite(key, data, meta, (EventKind::Transitioned, now))?;
        let stored = self.stored(key).ok_or(ServiceError::Unavailable)?;
        Ok((stored, logged))
    }

    /// Cancels a schedule that has not ended.
    pub(crate) fn schedule_delete(
        &mut self,
        key: &Key,
        expect: &Expect,
        (now, wall): (u64, u64),
    ) -> Result<Logged, ServiceError> {
        let stored = self.stored(key).ok_or_else(not_found)?;
        expect.check(&stored.state, stored.version)?;
        if terminal(&stored.state) {
            return Err(rejected("ERR flow is terminal; use FLOW.REWIND"));
        }
        let entry = self.hot(key).ok_or_else(not_found)?;
        let mut data = data_of(&entry.record);
        data.state = b"cancelled".to_vec();
        data.lease = None;
        data.run_state = None;
        data.next_run_at = None;
        let mut meta = self.meta_of(key);
        meta.terminal_until = Some(now.max(wall).saturating_add(meta.retention_ttl_ms));
        self.rewrite(key, data, meta, (EventKind::Cancelled, now))
    }
}
