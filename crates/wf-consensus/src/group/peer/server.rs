//! The peer server: a member answers each connection's requests in order.
//! A snapshot's bytes follow its request, so they are read (into the
//! member's snapshot store, or dropped when it is refused) before the
//! request is answered, and the connection stays in step.

use std::collections::BTreeMap;
use std::sync::Arc;

use openraft::storage::RaftStateMachine;
use openraft::type_config::alias::{SnapshotMetaOf, VoteOf};
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Raft, ReadPolicy, Snapshot};
use tokio::net::{TcpListener, TcpStream};
use wf_ports::consensus::log::SnapshotStore;

use super::super::ballot::Ballot;
use super::super::snapshot::{self, Stored};
use super::super::wire::{self, Held, Recipient, Request, Response};
use super::super::{Config, snapshot::aside};
use super::{MAX_FRAME_BYTES, frame};

/// What a member serves with: the largest frame and its snapshot store.
pub(in super::super) type Serving = (u32, Arc<dyn SnapshotStore>);

/// The answer to a vote request while this member may not vote.
const NOT_VOTING: &str = "member catching up: no vote";

/// Serves Raft requests from other members to `raft`, member `id` of the
/// group `peers` lists and a member that votes, keeping the snapshots it
/// receives in `store`, until the listener fails.
pub async fn serve<SM>(
    listener: TcpListener,
    raft: Raft<Config, SM>,
    (peers, id): (BTreeMap<u64, String>, u64),
    store: Arc<dyn SnapshotStore>,
) where
    SM: RaftStateMachine<Config, SnapshotData = Stored>,
{
    let own = Recipient {
        group: wire::group_id(&peers),
        member: id,
    };
    let ballot = Arc::new(Ballot::new(false));
    serving(listener, raft, (ballot, own), (MAX_FRAME_BYTES, store)).await;
}

/// Serves Raft requests for `own` to `raft`, which votes as `ballot`
/// allows, in frames of at most the limit `serving` names, keeping the
/// snapshots it receives in its store.
pub(in super::super) async fn serving<SM>(
    listener: TcpListener,
    raft: Raft<Config, SM>,
    (ballot, own): (Arc<Ballot>, Recipient),
    serving: Serving,
) where
    SM: RaftStateMachine<Config, SnapshotData = Stored>,
{
    // The connections stop with the server, so a peer that still holds one
    // to a stopped member reconnects to the next.
    let mut connections = tokio::task::JoinSet::new();
    while let Ok((stream, _peer)) = listener.accept().await {
        while connections.try_join_next().is_some() {}
        if stream.set_nodelay(true).is_err() {
            continue;
        }
        let served = (Arc::clone(&ballot), own);
        let with = (serving.0, Arc::clone(&serving.1));
        connections.spawn(connection(stream, raft.clone(), served, with));
    }
}

async fn connection<SM>(
    mut stream: TcpStream,
    raft: Raft<Config, SM>,
    (ballot, own): (Arc<Ballot>, Recipient),
    (limit, store): Serving,
) where
    SM: RaftStateMachine<Config, SnapshotData = Stored>,
{
    while let Ok(body) = frame::read_frame(&mut stream, limit).await {
        let (response, follows) = match wire::decode_request(&body) {
            Ok((recipient, Request::Snapshot(vote, meta, length))) if recipient == own => {
                let Ok(stored) = receive(&mut stream, (&store, &meta, length), limit).await else {
                    return;
                };
                (install(&raft, vote, meta, stored).await, None)
            }
            Ok((recipient, Request::Snapshot(_, _, length))) => {
                if frame::skip(&mut stream, length, limit).await.is_err() {
                    return;
                }
                (Response::Failed(misdirected(recipient, own)), None)
            }
            Ok((recipient, _)) if recipient != own => {
                (Response::Failed(misdirected(recipient, own)), None)
            }
            Ok((_, request)) => handle(&raft, &ballot, request).await,
            Err(error) => (Response::Failed(error.to_string()), None),
        };
        let Ok(bytes) = wire::encode_response(&response) else {
            return;
        };
        if frame::write_frame(&mut stream, &bytes).await.is_err() {
            return;
        }
        if let Some(follows) = follows {
            let opened = aside(move || follows.open()).await;
            let sent = match opened {
                Ok(source) => frame::send_source(&mut stream, source, limit).await,
                Err(error) => Err(error),
            };
            // A snapshot not sent whole leaves the connection out of step.
            if sent.is_err() {
                return;
            }
        }
    }
}

/// The refusal of a request meant for another group or member.
fn misdirected(recipient: Recipient, own: Recipient) -> String {
    format!(
        "a request for member {} of group {:016x} reached member {} of group {:016x}",
        recipient.member, recipient.group, own.member, own.group
    )
}

/// Receives a snapshot with `meta` of `length` bytes into `store`.
async fn receive(
    stream: &mut TcpStream,
    (store, meta, length): (&Arc<dyn SnapshotStore>, &SnapshotMetaOf<Config>, u64),
    limit: u32,
) -> std::io::Result<Stored> {
    let id = snapshot::id_of(meta);
    let opening = Arc::clone(store);
    let sink = aside(move || opening.create(id).map_err(std::io::Error::other)).await?;
    let length = frame::receive_into(stream, sink, length, limit).await?;
    Ok(Stored::new(Arc::clone(store), id, length))
}

async fn install<SM>(
    raft: &Raft<Config, SM>,
    vote: VoteOf<Config>,
    meta: SnapshotMetaOf<Config>,
    stored: Stored,
) -> Response
where
    SM: RaftStateMachine<Config, SnapshotData = Stored>,
{
    let snapshot = Snapshot {
        meta,
        snapshot: stored,
    };
    raft.install_full_snapshot(vote, snapshot)
        .await
        .map_or_else(
            |error| Response::Failed(error.to_string()),
            Response::Snapshot,
        )
}

/// The answer to `request`, and the snapshot whose bytes follow it (a
/// fetched one).
async fn handle<SM>(
    raft: &Raft<Config, SM>,
    ballot: &Ballot,
    request: Request,
) -> (Response, Option<Stored>)
where
    SM: RaftStateMachine<Config, SnapshotData = Stored>,
{
    let failed = |error: &dyn std::fmt::Display| Response::Failed(error.to_string());
    let response = match request {
        Request::Append(append) => {
            ballot.hear(&append);
            raft.append_entries(append)
                .await
                .map_or_else(|error| failed(&error), Response::Append)
        }
        // A member that may have lost its log gives no vote (ADR-019): the
        // candidate counts it as unreachable.
        Request::Vote(_) | Request::PreVote(_) if !ballot.votes() => {
            Response::Failed(NOT_VOTING.to_owned())
        }
        Request::Vote(vote) => raft
            .vote(vote)
            .await
            .map_or_else(|error| failed(&error), Response::Vote),
        Request::PreVote(vote) => raft
            .pre_vote(vote)
            .await
            .map_or_else(|error| failed(&error), Response::Vote),
        Request::Status => {
            let metrics = raft.metrics();
            let metrics = metrics.borrow_watched();
            Response::Status(Held {
                last_log: metrics.last_log_index,
                term: metrics.current_term,
            })
        }
        // Read with its bytes by `connection`.
        Request::Snapshot(..) => failed(&"snapshot without its bytes"),
        Request::Newest { after, upto } => return own_newest(raft, after, upto).await,
        // The leader confirms with a quorum that it still leads; a member
        // that does not lead refuses.
        Request::ReadIndex => raft
            .get_read_log_id(ReadPolicy::ReadIndex)
            .await
            .map_or_else(
                |error| failed(&error),
                |(read, _)| Response::ReadIndex(read.index()),
            ),
    };
    (response, None)
}

/// Whether a snapshot whose last index is `index` answers a request for
/// one past `after` and at most `upto`.
pub(super) fn wanted(index: Option<u64>, after: u64, upto: u64) -> bool {
    index.is_some_and(|index| index > after && index <= upto)
}

/// This member's newest snapshot if its last index is past `after` and at
/// most `upto`: its metadata and length, then its bytes; its index is
/// looked up first, so a request it declines copies nothing.
async fn own_newest<SM>(
    raft: &Raft<Config, SM>,
    after: u64,
    upto: u64,
) -> (Response, Option<Stored>)
where
    SM: RaftStateMachine<Config, SnapshotData = Stored>,
{
    let taken = raft.metrics().borrow_watched().snapshot;
    if !wanted(taken.map(|log_id| log_id.index), after, upto) {
        return (Response::Newest(None), None);
    }
    let snapshot = match raft.get_snapshot().await {
        Ok(snapshot) => snapshot.filter(|snapshot| {
            let index = snapshot.meta.last_log_id.map(|log_id| log_id.index);
            wanted(index, after, upto)
        }),
        Err(error) => return (Response::Failed(error.to_string()), None),
    };
    match snapshot {
        Some(Snapshot { meta, snapshot }) => (
            Response::Newest(Some((meta, snapshot.length()))),
            Some(snapshot),
        ),
        None => (Response::Newest(None), None),
    }
}
