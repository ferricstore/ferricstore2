//! How a member that starts without a log joins its group (ADR-019): it
//! asks every other member what it holds before it votes (`Ballot`), and
//! the lowest member of a new group forms it.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use openraft::Raft;
use openraft::storage::RaftStateMachine;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use tokio::net::TcpListener;

use super::Config;
use super::ballot::Ballot;
use super::peer;
use super::wire::Held;

/// What a member that started without a log makes of what the others hold.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Settled {
    /// The group is new; `form` when this is its lowest member and no other
    /// member holds any log.
    New { form: bool },
    /// Another member holds a log past a forming group's first entry, so
    /// this member lost its own; `term` is the highest the others know.
    Lost { term: u64 },
}

/// Settles member `id` of `peers`, which has no log, from what every other
/// member holds.
pub(super) fn settle(held: &[Held], id: u64, peers: &BTreeMap<u64, String>) -> Settled {
    if held
        .iter()
        .any(|held| held.last_log.is_some_and(|index| index > 0))
    {
        Settled::Lost {
            term: held.iter().map(|held| held.term).max().unwrap_or(0),
        }
    } else {
        Settled::New {
            form: peers.keys().next() == Some(&id)
                && held.iter().all(|held| held.last_log.is_none()),
        }
    }
}

/// Serves Raft requests to `raft`, member `id` of `peers`, until the
/// listener fails. A member that starts without a log (`fresh`) first
/// settles whether it may vote.
pub async fn serve_member<SM>(
    listener: TcpListener,
    raft: Raft<Config, SM>,
    peers: BTreeMap<u64, String>,
    id: u64,
    fresh: bool,
) where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    let ballot = Arc::new(Ballot::new(fresh));
    let settling = async {
        if fresh {
            join(&raft, &peers, id, &ballot).await;
        }
    };
    futures_util::future::join(
        peer::serving(listener, raft.clone(), Arc::clone(&ballot)),
        settling,
    )
    .await;
}

/// Asks every other member what it holds, again every half second until
/// each has answered once, then settles this member: one that lost its log
/// votes once it caught up; in a new group it votes, and the lowest member
/// forms it. The answers need not come together: a member's term only
/// grows, so an earlier answer still bounds the term it knew when this
/// member lost its log.
async fn join<SM>(raft: &Raft<Config, SM>, peers: &BTreeMap<u64, String>, id: u64, ballot: &Ballot)
where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    let mut held = BTreeMap::new();
    loop {
        for (member, address) in peers {
            if *member != id
                && !held.contains_key(member)
                && let Ok(answer) = peer::status(address).await
            {
                held.insert(*member, answer);
            }
        }
        if peers
            .keys()
            .all(|member| *member == id || held.contains_key(member))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let held: Vec<Held> = held.into_values().collect();
    match settle(&held, id, peers) {
        Settled::Lost { term } => {
            ballot.catch_up(term);
            caught_up(raft, ballot).await;
        }
        Settled::New { form } => {
            ballot.open();
            if form {
                let members: BTreeSet<u64> = peers.keys().copied().collect();
                // Another start of the same member may have formed it.
                let _formed = raft.initialize(members).await;
            }
        }
    }
}

/// Waits until `ballot` lets this member vote. The commit point it waits
/// for arrives with an append, which need not change this member's
/// metrics, so it also looks again every tenth of a second.
async fn caught_up<SM>(raft: &Raft<Config, SM>, ballot: &Ballot)
where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    let mut metrics = raft.metrics();
    loop {
        let applied = metrics
            .borrow_watched()
            .last_applied
            .map(|log_id| log_id.index);
        if ballot.applied(applied) {
            return;
        }
        if let Ok(Err(_closed)) =
            tokio::time::timeout(Duration::from_millis(100), metrics.changed()).await
        {
            return;
        }
    }
}
