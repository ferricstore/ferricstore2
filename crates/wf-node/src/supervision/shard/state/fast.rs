//! The queue fast path's create and complete on one shard (ADR-011): a
//! logical record each, decided by the reducer through `apply`.

use wf_application::commands::flow::value_codec;
use wf_application::commands::queue::{
    CompleteFlow, ItemOutcome, NewFlow, Partition, ServiceError,
};
use wf_domain::workflow::Outcome;

use super::super::command::Logged;
use super::ShardState;

impl ShardState {
    /// Creates one Flow; returns its outcome and the change to log.
    pub(crate) fn create(
        &mut self,
        now: u64,
        shape: (&[u8], &[u8], Option<u64>, u8),
        flow: NewFlow,
        partition: Partition,
    ) -> (ItemOutcome, Option<Logged>) {
        let (flow_type, state, run_at, priority) = shape;
        let explicit = matches!(partition, Partition::Named(_));
        if let Err(error) = self.fifo_guard((flow_type, state), false, priority, Some(explicit)) {
            return (Err(error), None);
        }
        let logged = Logged::Create {
            now,
            partition,
            id: flow.id,
            flow_type: flow_type.to_vec(),
            state: state.to_vec(),
            run_at,
            // Escaped like any stored value (audit gap G5).
            payload: flow.payload.map(value_codec::escape),
            priority,
        };
        settle(self.apply(&logged), logged)
    }

    /// Completes one claimed Flow; returns its outcome and the change to log.
    pub(crate) fn complete(
        &mut self,
        now: u64,
        completion: CompleteFlow,
        partition: Partition,
    ) -> (ItemOutcome, Option<Logged>) {
        // Past its active-time limit, the Flow fails for it (audit gap G21).
        if self.any_overdue(now) {
            let key = super::flow_key(partition.clone(), completion.id.clone());
            if self.overdue(&key, now) {
                let exceeded = ServiceError::rejected("ERR flow max_active_ms exceeded".to_owned());
                return (Err(exceeded), self.time_out_one(now, now, &key));
            }
        }
        let logged = Logged::Complete {
            now,
            partition,
            id: completion.id,
            token: completion.lease_token,
            fence: completion.fence,
            result: completion.result.map(value_codec::escape),
        };
        settle(self.apply(&logged), logged)
    }
}

/// An item outcome, and the change to log when the reducer changed the record.
fn settle(applied: Result<Outcome, ServiceError>, logged: Logged) -> (ItemOutcome, Option<Logged>) {
    match applied {
        Ok(Outcome::Changed) => (Ok(()), Some(logged)),
        Ok(_) => (Ok(()), None),
        Err(error) => (Err(error), None),
    }
}
