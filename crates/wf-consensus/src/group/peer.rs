//! Raft traffic between members over TCP (ADR-018), separate from client
//! listeners (ADR-001).
//!
//! `OpenRaft` gives each replication stream and each vote its own network
//! client and calls it one request at a time, so a client keeps one
//! connection and waits for each answer: a frame is a `u32` length and a
//! [`wire`] body. A member serves each connection's requests in order. A
//! failed connection is dropped and the next request reconnects; snapshots
//! are not sent yet (phase 3).

use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use openraft::error::{RPCError, ReplicationClosed, StreamingError, Unreachable};
use openraft::network::{RPCOption, RaftNetworkFactory, RaftNetworkV2};
use openraft::raft::{
    AppendEntriesRequest, AppendResult, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::storage::RaftStateMachine;
use openraft::type_config::alias::{SnapshotOf, VoteOf};
use openraft::{AnyError, OptionalSend, Raft};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::Config;
use super::wire::{self, Request, Response};

/// Largest frame a member accepts.
const MAX_FRAME_BYTES: u32 = 256 * 1024 * 1024;
/// How long a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

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
    SM: RaftStateMachine<Config>,
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
    SM: RaftStateMachine<Config>,
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
    SM: RaftStateMachine<Config>,
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
    }
}

/// The members' Raft addresses: `OpenRaft`'s network factory. `SD` is the
/// state machine's snapshot data.
pub struct Peers<SD> {
    addresses: Arc<BTreeMap<u64, SocketAddr>>,
    snapshot: PhantomData<fn() -> SD>,
}

impl<SD> Peers<SD> {
    /// The network to `addresses`, by member id.
    #[must_use]
    pub fn new(addresses: BTreeMap<u64, SocketAddr>) -> Self {
        Self {
            addresses: Arc::new(addresses),
            snapshot: PhantomData,
        }
    }
}

impl<SD: OptionalSend + 'static> RaftNetworkFactory<Config> for Peers<SD> {
    type Network = Link<SD>;

    fn new_client(&mut self, target: u64, (): &()) -> impl Future<Output = Link<SD>> + Send {
        ready(Link {
            address: self.addresses.get(&target).copied(),
            stream: None,
            snapshot: PhantomData,
        })
    }
}

/// One client's connection to a member.
pub struct Link<SD> {
    address: Option<SocketAddr>,
    stream: Option<TcpStream>,
    snapshot: PhantomData<fn() -> SD>,
}

impl<SD> Link<SD> {
    async fn connect(&self) -> io::Result<TcpStream> {
        let address = self
            .address
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
        option: &RPCOption,
    ) -> Result<Response, RPCError<Config>> {
        match tokio::time::timeout(option.hard_ttl(), self.exchange(&request)).await {
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

fn mismatched() -> RPCError<Config> {
    RPCError::Unreachable(unreachable(&"mismatched peer response"))
}

impl<SD: OptionalSend + 'static> RaftNetworkV2<Config> for Link<SD> {
    type SnapshotData = SD;

    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<Config>,
        option: RPCOption,
    ) -> Result<AppendResult<Config>, RPCError<Config>> {
        match self.call(Request::Append(rpc), &option).await? {
            Response::Append(result) => Ok(result),
            _ => Err(mismatched()),
        }
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<Config>,
        option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        match self.call(Request::Vote(rpc), &option).await? {
            Response::Vote(vote) => Ok(vote),
            _ => Err(mismatched()),
        }
    }

    async fn pre_vote(
        &mut self,
        rpc: VoteRequest<Config>,
        option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        match self.call(Request::PreVote(rpc), &option).await? {
            Response::Vote(vote) => Ok(vote),
            _ => Err(mismatched()),
        }
    }

    fn full_snapshot(
        &mut self,
        _vote: VoteOf<Config>,
        _snapshot: SnapshotOf<Config, SD>,
        _cancel: impl Future<Output = ReplicationClosed> + OptionalSend + 'static,
        _option: RPCOption,
    ) -> impl Future<Output = Result<SnapshotResponse<Config>, StreamingError<Config>>> + Send {
        ready(Err(StreamingError::Unreachable(unreachable(
            &"snapshots are not sent between members yet",
        ))))
    }
}
