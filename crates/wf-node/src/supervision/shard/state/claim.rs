//! Claim selection on one shard (ADR-013): due Flows, expired leases, and
//! Elixir's order between them.

use std::cmp::Reverse;

use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, Partition};
use wf_domain::workflow::Outcome;

use super::credit::CreditPool;
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

/// What a shard could claim now for a request, and how long until a
/// matching Flow falls due or a matching lease expires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Peeked {
    pub(crate) available: usize,
    pub(crate) wait_ms: Option<u64>,
}

/// Entries scanned for a partition match before the earliest due time of
/// the whole queue is reported instead.
const DUE_SCAN: usize = 64;

/// Index entries a request may take, in the order it takes them.
fn modes(request: &ClaimFlows) -> &'static [Mode] {
    if request.running_only() {
        &[Mode::Expired]
    } else if request.reclaim.is_some() {
        &[Mode::Due, Mode::Expired]
    } else {
        &[Mode::Due]
    }
}

type Due = std::collections::BTreeMap<(u64, u64), Key>;

/// The earliest due time after `now` in one queue, preferring an entry in
/// an allowed partition among the first [`DUE_SCAN`].
fn next_due(
    due: &Due,
    now: u64,
    request: &ClaimFlows,
    partitions: Option<&[Partition]>,
) -> Option<u64> {
    let mut later = due.range((now.saturating_add(1), 0)..);
    let first = later.clone().next().map(|((at, _), _)| *at)?;
    let allowed = later
        .by_ref()
        .take(DUE_SCAN)
        .find(|(_, key)| request.allows(partitions, &key.0))
        .map(|((at, _), _)| *at);
    Some(allowed.unwrap_or(first))
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
        (limit, credits): (usize, Option<&CreditPool>),
        partitions: Option<&[Partition]>,
    ) -> Taken {
        let now = request.now_ms.unwrap_or(now);
        let mut taken = Taken::default();
        let mut step = |state: &mut Self, mode: Mode, wanted: usize| {
            let room = limit.saturating_sub(taken.claimed.len()).min(wanted);
            if room > 0 {
                state.take(now, (request, credits), mode, room, partitions, &mut taken);
            }
        };
        if request.running_only() {
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

    /// Counts what [`Self::claim`] could take now, up to `limit`, without
    /// changing anything.
    pub(crate) fn peek(
        &self,
        now: u64,
        request: &ClaimFlows,
        limit: usize,
        partitions: Option<&[Partition]>,
    ) -> Peeked {
        let now = request.now_ms.unwrap_or(now);
        let mut available = 0_usize;
        let mut next: Option<u64> = None;
        for mode in modes(request) {
            available = available.saturating_add(
                self.candidates(now, request, *mode, limit, partitions)
                    .len(),
            );
            let earliest = self
                .matching(request, *mode)
                .filter_map(|(_, due)| next_due(due, now, request, partitions))
                .min();
            next = match (next, earliest) {
                (Some(earlier), Some(at)) => Some(earlier.min(at)),
                (earlier, at) => earlier.or(at),
            };
        }
        Peeked {
            available: available.min(limit),
            wait_ms: next.map(|at| at.saturating_sub(now)),
        }
    }

    /// Queues of `mode` whose type, state and priority match the request.
    fn matching<'a>(
        &'a self,
        request: &'a ClaimFlows,
        mode: Mode,
    ) -> impl Iterator<Item = (&'a super::Queue, &'a Due)> {
        self.queues.iter().filter(move |(queue, _)| {
            let running = queue.state == RUNNING;
            queue.flow_type == request.flow_type
                && match mode {
                    Mode::Expired => running,
                    Mode::Due => {
                        !running
                            && (request.states.is_empty() || request.states.contains(&queue.state))
                    }
                }
                && request
                    .priority
                    .is_none_or(|priority| priority == queue.priority)
        })
    }

    fn take(
        &mut self,
        now: u64,
        (request, credits): (&ClaimFlows, Option<&CreditPool>),
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
            // A governed claim stamps the Flow with its credit and logs the
            // Flow's after-image instead.
            let stamped = credits
                .and_then(CreditPool::take)
                .is_some_and(|credit| self.stamp_credit(&key, credit));
            let change = if stamped {
                self.after_image_events(&key, 1).unwrap_or(change)
            } else {
                change
            };
            let record = request
                .records
                .as_ref()
                .and_then(|include| self.record_term(&key, include));
            if let Some(entry) = self.hot(&key) {
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
                    record,
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
        for (queue, due) in self.matching(request, mode) {
            if mode == Mode::Due
                && let Some(heads) = self.lane_heads(queue, now, request, partitions)
            {
                found.extend(
                    heads
                        .into_iter()
                        .map(|(slot, key)| (Reverse(queue.priority), slot, key)),
                );
                continue;
            }
            let eligible = due
                .range(..=(now, u64::MAX))
                .filter(|(_, key)| request.allows(partitions, &key.0))
                .take(limit)
                .map(|(slot, key)| (Reverse(queue.priority), *slot, key));
            found.extend(eligible);
        }
        found.sort_unstable_by_key(|(priority, slot, _)| (*priority, *slot));
        found.truncate(limit);
        found
            .into_iter()
            .filter_map(|(_, _, key)| {
                let record = &self.hot(key)?.record;
                Some((key.clone(), record.state().to_vec(), record.fence().get()))
            })
            .collect()
    }
}
