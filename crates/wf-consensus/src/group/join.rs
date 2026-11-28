//! How a member that starts without the mark that it votes joins its group
//! (ADR-019): it asks every other member what it holds before it votes
//! (`Ballot`), and the lowest voter of a new group forms it with the voters
//! alone (ADR-020). The mark is
//! saved once it may vote: a member stopped before its first entry votes
//! when it starts again, and one whose storage was lost, or that stopped
//! while catching up, asks again.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use openraft::Raft;
use openraft::storage::RaftStateMachine;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use tokio::net::TcpListener;
use wf_ports::consensus::log::{ConsensusLog, SnapshotStore};

use super::Config;
use super::ballot::Ballot;
use super::peer::{self, Answering};
use super::wire::{self, Held, Recipient};

/// What a member that started without a log makes of what the others hold.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Settled {
    /// The group is new; `forms` names its voters when this member is the
    /// lowest of them and no other member holds any log.
    New { forms: Option<BTreeSet<u64>> },
    /// Another member holds a log past a forming group's first entry, so
    /// this member lost its own; `term` is the highest the others know.
    Lost { term: u64 },
}

/// Settles member `id` of `peers`, which has no log and joins as a voter
/// when `voter`, from what every other member holds (`held`, by member).
pub(super) fn settle(
    held: &BTreeMap<u64, Held>,
    (id, voter): (u64, bool),
    peers: &BTreeMap<u64, String>,
) -> Settled {
    if held
        .values()
        .any(|held| held.last_log.is_some_and(|index| index > 0))
    {
        return Settled::Lost {
            term: held.values().map(|held| held.term).max().unwrap_or(0),
        };
    }
    let voters: BTreeSet<u64> = peers
        .keys()
        .copied()
        .filter(|member| {
            if *member == id {
                voter
            } else {
                held.get(member).is_some_and(|held| held.voter)
            }
        })
        .collect();
    let forms = voters.first() == Some(&id) && held.values().all(|held| held.last_log.is_none());
    Settled::New {
        forms: forms.then_some(voters),
    }
}

/// Serves Raft requests to `raft`, member `id` of `peers` that joins as a
/// voter when `voter`, keeping `log`
/// and its snapshots in `store`, in frames of at most `limit` bytes, until
/// the listener fails. A member
/// whose log lacks the mark that it votes first settles whether it may
/// vote, then saves the mark.
pub async fn serve_member<SM>(
    listener: TcpListener,
    raft: Raft<Config, SM>,
    (peers, id, voter): (BTreeMap<u64, String>, u64, bool),
    (log, store, limit): (Arc<dyn ConsensusLog>, Arc<dyn SnapshotStore>, u32),
) where
    SM: RaftStateMachine<Config, SnapshotData = super::Stored>,
{
    let fresh = !log.voter();
    let ballot = Arc::new(Ballot::new(fresh));
    let own = Recipient {
        group: wire::group_id(&peers),
        member: id,
    };
    let settling = async {
        if fresh {
            join(&raft, &peers, (own, voter), &ballot).await;
            // `join` also returns when the member stops. A mark that is not
            // saved only makes the next start ask again.
            if ballot.votes() {
                let _saved = log.save_voter();
            }
        }
    };
    futures_util::future::join(
        peer::serving(
            listener,
            raft.clone(),
            Answering {
                ballot: Arc::clone(&ballot),
                own,
                voter,
            },
            (limit, store),
        ),
        settling,
    )
    .await;
}

/// Asks every other member what it holds, again every half second until
/// each has answered once, then settles this member: one that lost its log
/// votes once it caught up; in a new group it votes, and the lowest voter
/// forms it with the voters. The answers need not come together: a member's term only
/// grows, so an earlier answer still bounds the term it knew when this
/// member lost its log.
async fn join<SM>(
    raft: &Raft<Config, SM>,
    peers: &BTreeMap<u64, String>,
    (own, voter): (Recipient, bool),
    ballot: &Ballot,
) where
    SM: RaftStateMachine<Config, SnapshotData = super::Stored>,
{
    let id = own.member;
    let mut held = BTreeMap::new();
    loop {
        for (member, address) in peers {
            let recipient = Recipient {
                member: *member,
                ..own
            };
            if *member != id
                && !held.contains_key(member)
                && let Ok(answer) = peer::status(address, recipient).await
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
    match settle(&held, (id, voter), peers) {
        Settled::Lost { term } => {
            ballot.catch_up(term);
            caught_up(raft, ballot).await;
        }
        Settled::New { forms } => {
            ballot.open();
            if let Some(voters) = forms {
                // Another start of the same member may have formed it.
                let _formed = raft.initialize(voters).await;
            }
        }
    }
}

/// Waits until `ballot` lets this member vote. The commit point it waits
/// for arrives with an append, which need not change this member's
/// metrics, so it also looks again every tenth of a second.
async fn caught_up<SM>(raft: &Raft<Config, SM>, ballot: &Ballot)
where
    SM: RaftStateMachine<Config, SnapshotData = super::Stored>,
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
