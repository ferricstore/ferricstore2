//! Ordered proposals (ADR-018): a leader's shard rounds become entries in
//! the order they arrive, and each round's completion runs once its entry is
//! committed (a majority holds it durably), after the earlier rounds of the
//! same lane completed. A round without changes still waits for its lane's
//! earlier rounds, as its answer may read their changes. A round whose
//! entry is refused or lost reports `false`.

use std::collections::HashMap;

use openraft::Raft;
use openraft::impls::ProgressResponder;
use tokio::sync::{mpsc, oneshot};

use super::machine::{Bridge, wrap};
use super::{Config, Proposal};

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
