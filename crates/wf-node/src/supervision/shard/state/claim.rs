//! Claim selection on one shard (ADR-013): due Flows, expired leases, and
//! Elixir's order between them.

use std::cmp::Reverse;

use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, Partition};
use wf_domain::workflow::Outcome;

use super::{Key, Logged, RUNNING, ShardState};

/// Which index entries a claim step takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Scheduled unleased Flows whose due time has come.
    Due,
    /// Running Flows whose lease deadline has passed.
    Expired,
}

/// Claimed Flows and the changes to log, in claim order.
#[derive(Default)]
pub(crate) struct Taken {
    pub(crate) claimed: Vec<ClaimedFlow>,
    pub(crate) logged: Vec<Logged>,
}

/// Reclaimed before due Flows: `ceil(limit * ratio / 100)`, at least one.
fn first_reclaim(limit: usize, ratio: u8) -> usize {
    limit
        .saturating_mul(usize::from(ratio.min(100)))
        .saturating_add(99)
        .checked_div(100)
        .unwrap_or(0)
        .max(1)
        .min(limit)
}

impl ShardState {
    /// Claims up to `limit` Flows in Elixir's order; returns them and the
    /// changes to log.
    pub(crate) fn claim(
        &mut self,
        now: u64,
        request: &ClaimFlows,
        limit: usize,
        partitions: Option<&[Partition]>,
    ) -> Taken {
        let mut taken = Taken::default();
        let mut step = |state: &mut Self, mode: Mode, wanted: usize| {
            let room = limit.saturating_sub(taken.claimed.len()).min(wanted);
            if room > 0 {
                state.take(now, request, mode, room, partitions, &mut taken);
            }
        };
        if request.state.as_deref() == Some(RUNNING) {
            step(self, Mode::Expired, limit);
        } else {
            match request.reclaim {
                None => step(self, Mode::Due, limit),
                Some(ratio) if ratio == 0 || limit == 1 => {
                    step(self, Mode::Due, limit);
                    step(self, Mode::Expired, limit);
                }
                Some(ratio) => {
                    step(self, Mode::Expired, first_reclaim(limit, ratio));
                    step(self, Mode::Due, limit);
                    step(self, Mode::Expired, limit);
                }
            }
        }
        taken
    }

    fn take(
        &mut self,
        now: u64,
        request: &ClaimFlows,
        mode: Mode,
        limit: usize,
        partitions: Option<&[Partition]>,
        taken: &mut Taken,
    ) {
        for (key, expected_state, fence) in self.candidates(now, request, mode, limit, partitions) {
            let lease_fence = fence.saturating_add(1);
            let worker = String::from_utf8_lossy(&request.worker);
            let change = Logged::Claim {
                now,
                partition: key.0.clone(),
                id: key.1.clone(),
                expected_state,
                worker: request.worker.clone(),
                token: format!("{worker}:{now}:{lease_fence}").into_bytes(),
                lease_ms: request.lease_ms,
                reclaim: mode == Mode::Expired,
            };
            if self.apply(&change) != Ok(Outcome::Changed) {
                continue;
            }
            if let Some(entry) = self.records.get(&key) {
                taken.claimed.push(ClaimedFlow {
                    id: key.1.clone(),
                    partition: key.0.name(),
                    lease_token: entry
                        .record
                        .lease()
                        .map(|lease| lease.token.as_bytes().to_vec())
                        .unwrap_or_default(),
                    fence: entry.record.fence().get(),
                    run_state: entry
                        .record
                        .run_state()
                        .map(<[u8]>::to_vec)
                        .unwrap_or_default(),
                });
                taken.logged.push(change);
            }
        }
    }

    /// Index entries of `mode` matching the request whose time has come:
    /// higher priority first, then due time or lease deadline, then creation
    /// order.
    fn candidates(
        &self,
        now: u64,
        request: &ClaimFlows,
        mode: Mode,
        limit: usize,
        partitions: Option<&[Partition]>,
    ) -> Vec<(Key, Vec<u8>, u64)> {
        let mut found: Vec<(Reverse<u8>, (u64, u64), &Key)> = Vec::new();
        let queues = self.queues.iter().filter(|(queue, _)| {
            let running = queue.state == RUNNING;
            queue.flow_type == request.flow_type
                && match mode {
                    Mode::Expired => running,
                    Mode::Due => {
                        !running
                            && request
                                .state
                                .as_ref()
                                .is_none_or(|state| *state == queue.state)
                    }
                }
                && request
                    .priority
                    .is_none_or(|priority| priority == queue.priority)
        });
        for (queue, due) in queues {
            let eligible = due
                .range(..=(now, u64::MAX))
                .filter(|(_, key)| partitions.is_none_or(|allowed| allowed.contains(&key.0)))
                .take(limit)
                .map(|(slot, key)| (Reverse(queue.priority), *slot, key));
            found.extend(eligible);
        }
        found.sort_unstable_by_key(|(priority, slot, _)| (*priority, *slot));
        found.truncate(limit);
        found
            .into_iter()
            .filter_map(|(_, _, key)| {
                let record = &self.records.get(key)?.record;
                Some((key.clone(), record.state().to_vec(), record.fence().get()))
            })
            .collect()
    }
}
