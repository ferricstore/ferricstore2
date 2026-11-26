//! Raft traffic between members over TCP (ADR-018), separate from client
//! listeners (ADR-001).
//!
//! `OpenRaft` gives each replication stream and each vote its own network
//! client and calls it one request at a time, so a client keeps one
//! connection and waits for each answer: a frame is a `u32` length and a
//! [`wire`] body. A member serves each connection's requests in order. A
//! failed connection is dropped and the next request reconnects. A snapshot
//! for a member the log no longer reaches goes in one frame (phase 3; at
//! most the frame limit), with a longer deadline. Each request names its
//! group and member; a member answers one meant for another with a
//! refusal that names both.

use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::io;
use std::sync::Arc;
use std::time::Duration;

use openraft::error::{RPCError, ReplicationClosed, StreamingError, Unreachable};
use openraft::network::{RPCOption, RaftNetworkFactory, RaftNetworkV2};
use openraft::raft::{
    AppendEntriesRequest, AppendResult, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::storage::RaftStateMachine;
use openraft::type_config::alias::{SnapshotOf, VoteOf};
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{AnyError, OptionalSend, Raft};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::Config;
use super::ballot::Ballot;
use super::wire::{self, Held, Recipient, Request, Response};

/// Largest frame a member accepts.
const MAX_FRAME_BYTES: u32 = 256 * 1024 * 1024;
/// How long a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// How long sending and installing a snapshot may take.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(120);
/// The answer to a vote request while this member may not vote.
const NOT_VOTING: &str = "member catching up: no vote";

fn unreachable(error: &impl std::fmt::Display) -> Unreachable<Config> {
    Unreachable::new(&AnyError::error(error.to_string()))
}

async fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let length = stream.read_u32().await?;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "peer frame too large",
        ));
    }
    let mut body = vec![0; usize::try_from(length).map_err(io::Error::other)?];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

async fn write_frame(stream: &mut TcpStream, body: &[u8]) -> io::Result<()> {
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(body.len().saturating_add(4));
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(body);
    stream.write_all(&frame).await
}

/// Serves Raft requests from other members to `raft`, member `id` of the
/// group `peers` lists and a member that votes, until the listener fails.
pub async fn serve<SM>(
    listener: TcpListener,
    raft: Raft<Config, SM>,
    peers: BTreeMap<u64, String>,
    id: u64,
) where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    let own = Recipient {
        group: wire::group_id(&peers),
        member: id,
    };
    serving(listener, raft, Arc::new(Ballot::new(false)), own).await;
}

/// Serves Raft requests for `own` to `raft`, which votes as `ballot`
/// allows.
pub(super) async fn serving<SM>(
    listener: TcpListener,
    raft: Raft<Config, SM>,
    ballot: Arc<Ballot>,
    own: Recipient,
) where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    // The connections stop with the server, so a peer that still holds one
    // to a stopped member reconnects to the next.
    let mut connections = tokio::task::JoinSet::new();
    while let Ok((stream, _peer)) = listener.accept().await {
        while connections.try_join_next().is_some() {}
        if stream.set_nodelay(true).is_err() {
            continue;
        }
        connections.spawn(connection(stream, raft.clone(), Arc::clone(&ballot), own));
    }
}

async fn connection<SM>(
    mut stream: TcpStream,
    raft: Raft<Config, SM>,
    ballot: Arc<Ballot>,
    own: Recipient,
) where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    while let Ok(body) = read_frame(&mut stream).await {
        let response = match wire::decode_request(&body) {
            Ok((recipient, _)) if recipient != own => Response::Failed(misdirected(recipient, own)),
            Ok((_, request)) => handle(&raft, &ballot, request).await,
            Err(error) => Response::Failed(error.to_string()),
        };
        let Ok(bytes) = wire::encode_response(&response) else {
            return;
        };
        if write_frame(&mut stream, &bytes).await.is_err() {
            return;
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

async fn handle<SM>(raft: &Raft<Config, SM>, ballot: &Ballot, request: Request) -> Response
where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    let failed = |error: &dyn std::fmt::Display| Response::Failed(error.to_string());
    match request {
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
        Request::Snapshot(vote, snapshot) => raft
            .install_full_snapshot(vote, snapshot)
            .await
            .map_or_else(|error| failed(&error), Response::Snapshot),
    }
}

/// The members' Raft addresses: `OpenRaft`'s network factory.
pub struct Peers {
    addresses: Arc<BTreeMap<u64, String>>,
    group: u64,
}

impl Peers {
    /// The network to `addresses` (`host:port`, resolved on each
    /// connection), by member id: the group they list.
    #[must_use]
    pub fn new(addresses: BTreeMap<u64, String>) -> Self {
        Self {
            group: wire::group_id(&addresses),
            addresses: Arc::new(addresses),
        }
    }
}

impl RaftNetworkFactory<Config> for Peers {
    type Network = Link;

    fn new_client(&mut self, target: u64, (): &()) -> impl Future<Output = Link> + Send {
        ready(Link {
            address: self.addresses.get(&target).cloned(),
            recipient: Recipient {
                group: self.group,
                member: target,
            },
            stream: None,
        })
    }
}

/// One client's connection to a member.
pub struct Link {
    address: Option<String>,
    recipient: Recipient,
    stream: Option<TcpStream>,
}

impl Link {
    async fn connect(&self) -> io::Result<TcpStream> {
        let address = self
            .address
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown member"))?;
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(io::Error::other)??;
        stream.set_nodelay(true)?;
        Ok(stream)
    }

    async fn exchange(&mut self, request: &Request) -> io::Result<Response> {
        let body = wire::encode_request(self.recipient, request)?;
        let mut stream = if let Some(stream) = self.stream.take() {
            stream
        } else {
            self.connect().await?
        };
        write_frame(&mut stream, &body).await?;
        let answer = read_frame(&mut stream).await?;
        self.stream = Some(stream);
        wire::decode_response(&answer)
    }

    /// Sends `request` within the option's deadline; a failed or late
    /// exchange drops the connection.
    async fn call(
        &mut self,
        request: Request,
        deadline: Duration,
    ) -> Result<Response, RPCError<Config>> {
        match tokio::time::timeout(deadline, self.exchange(&request)).await {
            Ok(Ok(Response::Failed(text))) => Err(RPCError::Unreachable(unreachable(&text))),
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => {
                self.stream = None;
                Err(RPCError::Unreachable(unreachable(&error)))
            }
            Err(elapsed) => {
                self.stream = None;
                Err(RPCError::Unreachable(unreachable(&elapsed)))
            }
        }
    }
}

/// What `recipient`, at `address`, holds (ADR-019).
///
/// # Errors
/// The member cannot be reached, refuses the request or answers something
/// else.
pub(super) async fn status(address: &str, recipient: Recipient) -> io::Result<Held> {
    let mut link = Link {
        address: Some(address.to_owned()),
        recipient,
        stream: None,
    };
    let answer = tokio::time::timeout(CONNECT_TIMEOUT, link.exchange(&Request::Status))
        .await
        .map_err(io::Error::other)??;
    match answer {
        Response::Status(held) => Ok(held),
        _ => Err(io::Error::other("mismatched peer response")),
    }
}

fn mismatched() -> RPCError<Config> {
    RPCError::Unreachable(unreachable(&"mismatched peer response"))
}

impl RaftNetworkV2<Config> for Link {
    type SnapshotData = Vec<u8>;

    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<Config>,
        option: RPCOption,
    ) -> Result<AppendResult<Config>, RPCError<Config>> {
        match self.call(Request::Append(rpc), option.hard_ttl()).await? {
            Response::Append(result) => Ok(result),
            _ => Err(mismatched()),
        }
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<Config>,
        option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        match self.call(Request::Vote(rpc), option.hard_ttl()).await? {
            Response::Vote(vote) => Ok(vote),
            _ => Err(mismatched()),
        }
    }

    async fn pre_vote(
        &mut self,
        rpc: VoteRequest<Config>,
        option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        match self.call(Request::PreVote(rpc), option.hard_ttl()).await? {
            Response::Vote(vote) => Ok(vote),
            _ => Err(mismatched()),
        }
    }

    async fn full_snapshot(
        &mut self,
        vote: VoteOf<Config>,
        snapshot: SnapshotOf<Config, Vec<u8>>,
        _cancel: impl Future<Output = ReplicationClosed> + OptionalSend + 'static,
        option: RPCOption,
    ) -> Result<SnapshotResponse<Config>, StreamingError<Config>> {
        let deadline = option.hard_ttl().max(SNAPSHOT_TIMEOUT);
        match self.call(Request::Snapshot(vote, snapshot), deadline).await {
            Ok(Response::Snapshot(taken)) => Ok(taken),
            Ok(_) => Err(StreamingError::Unreachable(unreachable(
                &"mismatched peer response",
            ))),
            Err(RPCError::Unreachable(error)) => Err(StreamingError::Unreachable(error)),
            Err(error) => Err(StreamingError::Unreachable(unreachable(&error))),
        }
    }
}
