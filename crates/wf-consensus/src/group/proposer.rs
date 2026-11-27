//! Ordered proposals (ADR-018): a leader's shard rounds become entries in
//! the order they arrive, and each round's completion runs once its entry is
//! committed (a majority holds it durably), after the earlier rounds of the
//! same lane completed. A round without changes still waits for its lane's
//! earlier rounds, as its answer may read their changes. A round whose
//! entry is refused or lost reports `false`.
//!
//! A leader proposes while a quorum acknowledged it within half its lease.
//! `OpenRaft` refuses a leader's writes once no quorum acknowledged it
//! within its lease, and the leader's shards applied a round before it is
//! proposed, so a refused round stops the node; a follower that pauses
//! (installing a snapshot, its disk stalled) while the other is down made
//! the leader stop. A round now waits for the quorum, up to [`HOLD`].

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::Duration;

use openraft::impls::ProgressResponder;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Instant, Raft, RaftMetrics};
use tokio::sync::{mpsc, oneshot};

use super::machine::{Bridge, wrap};
use super::{Config, Proposal};

/// How recently a quorum must have acknowledged this leader for it to
/// propose: half its lease (`election_timeout_max`, set in `member`), so a
/// proposal reaches the core before the lease lapses.
const FRESH: Duration = Duration::from_millis(500);
/// How long a round waits for a quorum before it is proposed anyway.
const HOLD: Duration = Duration::from_secs(30);

/// Whether a member with `metrics` may propose now: a quorum acknowledged
/// it within [`FRESH`], or it needs none (its group's only voter), or it
/// does not lead, so waiting gains nothing (its proposal is refused and its
/// round reported lost, as when leadership moved).
fn heard(metrics: &RaftMetrics<Config>) -> bool {
    metrics.current_leader != Some(metrics.id)
        || metrics.membership_config.membership().voter_ids().count() < 2
        || metrics
            .last_quorum_acked
            .as_ref()
            .is_some_and(|acked| acked.elapsed() < FRESH)
}

/// Waits until this member may propose ([`heard`]) or [`HOLD`] passed.
async fn quorum_heard(raft: &Raft<Config, Bridge>) {
    let mut metrics = raft.metrics();
    let until = tokio::time::Instant::now().checked_add(HOLD);
    loop {
        let heard = heard(&metrics.borrow_watched());
        if heard || until.is_none_or(|until| tokio::time::Instant::now() >= until) {
            return;
        }
        // An acknowledgment republishes the metrics; look again shortly
        // either way.
        if let Ok(Err(_closed)) =
            tokio::time::timeout(Duration::from_millis(20), metrics.changed()).await
        {
            return;
        }
    }
}

/// Called once with whether the round's entry committed.
pub type Committed = Box<dyn FnOnce(bool) + Send>;

struct Round {
    lane: u16,
    body: Option<Vec<u8>>,
    done: Committed,
}

/// Hands rounds to the group in order; clones share the queue.
#[derive(Clone)]
pub struct Proposer {
    rounds: mpsc::UnboundedSender<Round>,
}

impl Proposer {
    /// Proposes `body` (none for a round without changes) on `lane`.
    pub fn propose(&self, lane: u16, body: Option<Vec<u8>>, done: Committed) {
        if let Err(returned) = self.rounds.send(Round { lane, body, done }) {
            (returned.0.done)(false);
        }
    }
}

/// The proposer and the loop to run on the group's runtime.
pub(super) fn start(
    raft: Raft<Config, Bridge>,
    epoch: u64,
) -> (Proposer, impl Future<Output = ()> + Send + 'static) {
    let (rounds, queue) = mpsc::unbounded_channel();
    (Proposer { rounds }, run(raft, epoch, queue))
}

async fn run(raft: Raft<Config, Bridge>, epoch: u64, mut queue: mpsc::UnboundedReceiver<Round>) {
    let mut lanes: HashMap<u16, oneshot::Receiver<()>> = HashMap::new();
    while let Some(round) = queue.recv().await {
        let commit = match &round.body {
            Some(body) => {
                quorum_heard(&raft).await;
                let (responder, committed, _applied) = ProgressResponder::new();
                let proposal = Proposal(wrap(epoch, body));
                match raft.client_write_ff(proposal, Some(responder)).await {
                    Ok(()) => Some(committed),
                    Err(_) => None,
                }
            }
            None => None,
        };
        let proposed = round.body.is_none() || commit.is_some();
        let (finished, next) = oneshot::channel();
        let earlier = lanes.insert(round.lane, next);
        tokio::spawn(async move {
            let committed = match commit {
                Some(committed) => committed.await.is_ok(),
                None => proposed,
            };
            if let Some(earlier) = earlier {
                // A failed earlier round still lets the lane go on.
                let _done = earlier.await;
            }
            (round.done)(committed);
            let _next = finished.send(());
        });
    }
}
