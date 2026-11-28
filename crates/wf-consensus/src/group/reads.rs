//! Reads on any member (ADR-018): a member answers a read once it applied
//! everything the leader had committed when the read arrived. It asks the
//! leader for its read index (the leader first confirms with a quorum that
//! it still leads: `ReadPolicy::ReadIndex`), then waits for its own applied
//! position to reach that index. Reads that arrive while an ask is under
//! way share the next one, so a burst of reads costs one round trip.

use std::collections::BTreeMap;
use std::time::Duration;

use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Raft, ReadPolicy};
use tokio::sync::mpsc;

use super::Config;
use super::machine::Bridge;
use super::peer::{Link, read_index};
use super::wire::{self, Recipient};

/// How long a read waits for the leader's read index.
const ASK: Duration = Duration::from_secs(2);
/// How long a read waits for this member to apply through the read index.
const APPLY: Duration = Duration::from_secs(5);

/// Called once with whether the member may answer the read now.
pub type Current = Box<dyn FnOnce(bool) + Send>;

/// Hands reads to a member's reader; clones share it.
#[derive(Clone)]
pub struct Reader {
    reads: mpsc::UnboundedSender<Current>,
}

impl Reader {
    /// Calls `done` with `true` once this member applied everything the
    /// leader had committed when this call was made, or with `false` when
    /// that cannot be had: no leader known, the leader unreachable or no
    /// longer leading, or this member too far behind.
    pub fn when_current(&self, done: Current) {
        if let Err(returned) = self.reads.send(done) {
            (returned.0)(false);
        }
    }
}

/// Where a member's reads ask: its Raft, every member's Raft address, its
/// own id and the frame limit.
pub(super) struct Asking {
    pub(super) raft: Raft<Config, Bridge>,
    pub(super) peers: BTreeMap<u64, String>,
    pub(super) own: u64,
    pub(super) limit: u32,
}

/// The reader and the loop to run on the group's runtime.
pub(super) fn start(asking: Asking) -> (Reader, impl Future<Output = ()> + Send + 'static) {
    let (reads, waiting) = mpsc::unbounded_channel();
    (Reader { reads }, run(asking, waiting))
}

async fn run(asking: Asking, mut waiting: mpsc::UnboundedReceiver<Current>) {
    let group = wire::group_id(&asking.peers);
    let mut links: BTreeMap<u64, Link> = BTreeMap::new();
    while let Some(first) = waiting.recv().await {
        let mut batch = vec![first];
        while let Ok(next) = waiting.try_recv() {
            batch.push(next);
        }
        // Asked after every read of the batch arrived, so the answer
        // covers each of them.
        let current = match read_point(&asking, group, &mut links).await {
            Some(index) => asking
                .raft
                .wait(Some(APPLY))
                .applied_index_at_least(Some(index), "a read on this member")
                .await
                .is_ok(),
            None => false,
        };
        for done in batch {
            done(current);
        }
    }
}

/// The leader's read index: asked over a link kept per leader, or this
/// member's own when it leads. A kept connection is dead once the leader's
/// process restarted, and an exchange that fails drops it, so a failure on
/// one is asked again over a new one.
async fn read_point(asking: &Asking, group: u64, links: &mut BTreeMap<u64, Link>) -> Option<u64> {
    let leader = asking.raft.metrics().borrow_watched().current_leader?;
    if leader == asking.own {
        let (read, _) = asking
            .raft
            .get_read_log_id(ReadPolicy::ReadIndex)
            .await
            .ok()?;
        return Some(read.index());
    }
    let address = asking.peers.get(&leader)?;
    let link = links.entry(leader).or_insert_with(|| {
        let recipient = Recipient {
            group,
            member: leader,
        };
        Link::to(address.clone(), recipient, asking.limit)
    });
    let kept = link.connected();
    match read_index(link, ASK).await {
        Ok(index) => Some(index),
        Err(_) if kept => read_index(link, ASK).await.ok(),
        Err(_) => None,
    }
}
