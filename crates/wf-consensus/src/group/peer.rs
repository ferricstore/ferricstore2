//! Raft traffic between members over TCP (ADR-018), separate from client
//! listeners (ADR-001).
//!
//! `OpenRaft` gives each replication stream and each vote its own network
//! client and calls it one request at a time, so a client keeps one
//! connection and waits for each answer: a frame is a `u32` length and a
//! [`wire`] body. A member serves each connection's requests in order. A
//! failed connection is dropped and the next request reconnects. A snapshot
//! for a member the log no longer reaches goes in one frame (phase 3; at
//! most the frame limit), with a longer deadline.

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
use super::wire::{self, Request, Response};

/// Largest frame a member accepts.
const MAX_FRAME_BYTES: u32 = 256 * 1024 * 1024;
/// How long a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// How long sending and installing a snapshot may take.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(120);

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

/// Serves Raft requests from other members to `raft` until the listener
/// fails.
pub async fn serve<SM>(listener: TcpListener, raft: Raft<Config, SM>)
where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    while let Ok((stream, _peer)) = listener.accept().await {
        if stream.set_nodelay(true).is_err() {
            continue;
        }
        tokio::spawn(connection(stream, raft.clone()));
    }
}

async fn connection<SM>(mut stream: TcpStream, raft: Raft<Config, SM>)
where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    while let Ok(body) = read_frame(&mut stream).await {
        let response = match wire::decode_request(&body) {
            Ok(request) => handle(&raft, request).await,
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

async fn handle<SM>(raft: &Raft<Config, SM>, request: Request) -> Response
where
    SM: RaftStateMachine<Config, SnapshotData = Vec<u8>>,
{
    let failed = |error: &dyn std::fmt::Display| Response::Failed(error.to_string());
    match request {
        Request::Append(append) => raft
            .append_entries(append)
            .await
            .map_or_else(|error| failed(&error), Response::Append),
        Request::Vote(vote) => raft
            .vote(vote)
            .await
            .map_or_else(|error| failed(&error), Response::Vote),
        Request::PreVote(vote) => raft
            .pre_vote(vote)
            .await
            .map_or_else(|error| failed(&error), Response::Vote),
        Request::Status => {
            Response::Status(raft.metrics().borrow_watched().last_log_index.is_some())
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
}

impl Peers {
    /// The network to `addresses` (`host:port`, resolved on each
    /// connection), by member id.
    #[must_use]
    pub fn new(addresses: BTreeMap<u64, String>) -> Self {
        Self {
            addresses: Arc::new(addresses),
        }
    }
}

impl RaftNetworkFactory<Config> for Peers {
    type Network = Link;

    fn new_client(&mut self, target: u64, (): &()) -> impl Future<Output = Link> + Send {
        ready(Link {
            address: self.addresses.get(&target).cloned(),
            stream: None,
        })
    }
}

/// One client's connection to a member.
pub struct Link {
    address: Option<String>,
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
        let body = wire::encode_request(request)?;
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

/// Whether the member at `address` holds any log (ADR-019).
///
/// # Errors
/// The member cannot be reached or answers something else.
pub(super) async fn status(address: &str) -> io::Result<bool> {
    let mut link = Link {
        address: Some(address.to_owned()),
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
