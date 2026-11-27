//! One client's connection to a member. `OpenRaft` calls it one request at
//! a time, so it keeps one connection and waits for each answer; a failed
//! or late exchange drops the connection and the next reconnects. An
//! append whose entries do not fit one frame goes as several appends,
//! each following the one before; a snapshot's bytes stream from its file
//! in frames after its request.

use std::future::Future;
use std::io;
use std::time::Duration;

use openraft::error::{RPCError, ReplicationClosed, StreamingError};
use openraft::network::{RPCOption, RaftNetworkV2};
use openraft::raft::{
    AppendEntriesRequest, AppendResult, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::type_config::alias::{SnapshotOf, VoteOf};
use openraft::{OptionalSend, Snapshot};
use tokio::net::TcpStream;
use wf_ports::consensus::log::{SnapshotSink, SnapshotSource};

use super::super::Config;
use super::super::snapshot::{Stored, aside};
use super::super::wire::{self, Recipient, Request, Response};
use super::{CONNECT_TIMEOUT, SNAPSHOT_TIMEOUT, frame, unreachable};

/// One client's connection to a member.
pub struct Link {
    pub(super) address: Option<String>,
    pub(super) recipient: Recipient,
    pub(super) stream: Option<TcpStream>,
    /// The largest frame either side sends.
    pub(super) limit: u32,
}

/// What a request sends after its frame: a snapshot's bytes.
type Follows = Option<Box<dyn SnapshotSource>>;

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

    /// Sends the encoded request `body` and the snapshot bytes that follow
    /// it, and reads the answer.
    pub(super) async fn exchange(&mut self, body: &[u8], follows: Follows) -> io::Result<Response> {
        let mut stream = if let Some(stream) = self.stream.take() {
            stream
        } else {
            self.connect().await?
        };
        frame::write_frame(&mut stream, body).await?;
        if let Some(source) = follows {
            frame::send_source(&mut stream, source, self.limit).await?;
        }
        let answer = frame::read_frame(&mut stream, self.limit).await?;
        self.stream = Some(stream);
        wire::decode_response(&answer)
    }

    /// Reads the `length` snapshot bytes an answer announced into `sink`
    /// and finishes it; a failure drops the connection.
    pub(super) async fn receive_into(
        &mut self,
        sink: Box<dyn SnapshotSink>,
        length: u64,
    ) -> io::Result<u64> {
        let mut stream = self
            .stream
            .take()
            .ok_or_else(|| io::Error::other("no connection to read a snapshot from"))?;
        let length = frame::receive_into(&mut stream, sink, length, self.limit).await?;
        self.stream = Some(stream);
        Ok(length)
    }

    /// Sends the encoded request `body` (and `follows`) within `deadline`;
    /// a failed or late exchange drops the connection.
    async fn call(
        &mut self,
        body: &[u8],
        follows: Follows,
        deadline: Duration,
    ) -> Result<Response, RPCError<Config>> {
        match tokio::time::timeout(deadline, self.exchange(body, follows)).await {
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

    fn encode(&self, request: &Request) -> Result<Vec<u8>, RPCError<Config>> {
        wire::encode_request(self.recipient, request)
            .map_err(|error| RPCError::Unreachable(unreachable(&error)))
    }
}

fn mismatched() -> RPCError<Config> {
    RPCError::Unreachable(unreachable(&"mismatched peer response"))
}

impl RaftNetworkV2<Config> for Link {
    type SnapshotData = Stored;

    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<Config>,
        option: RPCOption,
    ) -> Result<AppendResult<Config>, RPCError<Config>> {
        let limit = usize::try_from(self.limit).unwrap_or(usize::MAX);
        let parts = wire::encode_append(self.recipient, &rpc, limit)
            .map_err(|error| RPCError::Unreachable(unreachable(&error)))?;
        let mut result = Ok(None);
        for part in parts {
            match self.call(&part, None, option.hard_ttl()).await? {
                Response::Append(Ok(matched)) => result = Ok(matched),
                // A part refused: the parts after it would not follow.
                Response::Append(Err(error)) => return Ok(Err(error)),
                _ => return Err(mismatched()),
            }
        }
        Ok(result)
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<Config>,
        option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        let body = self.encode(&Request::Vote(rpc))?;
        match self.call(&body, None, option.hard_ttl()).await? {
            Response::Vote(vote) => Ok(vote),
            _ => Err(mismatched()),
        }
    }

    async fn pre_vote(
        &mut self,
        rpc: VoteRequest<Config>,
        option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        let body = self.encode(&Request::PreVote(rpc))?;
        match self.call(&body, None, option.hard_ttl()).await? {
            Response::Vote(vote) => Ok(vote),
            _ => Err(mismatched()),
        }
    }

    async fn full_snapshot(
        &mut self,
        vote: VoteOf<Config>,
        snapshot: SnapshotOf<Config, Stored>,
        _cancel: impl Future<Output = ReplicationClosed> + OptionalSend + 'static,
        option: RPCOption,
    ) -> Result<SnapshotResponse<Config>, StreamingError<Config>> {
        let deadline = option.hard_ttl().max(SNAPSHOT_TIMEOUT);
        let streaming = |error: RPCError<Config>| match error {
            RPCError::Unreachable(error) => StreamingError::Unreachable(error),
            error => StreamingError::Unreachable(unreachable(&error)),
        };
        let Snapshot { meta, snapshot } = snapshot;
        let body = self
            .encode(&Request::Snapshot(vote, meta, snapshot.length()))
            .map_err(streaming)?;
        let source = aside(move || snapshot.open())
            .await
            .map_err(|error| StreamingError::Unreachable(unreachable(&error)))?;
        match self.call(&body, Some(source), deadline).await {
            Ok(Response::Snapshot(taken)) => Ok(taken),
            Ok(_) => Err(streaming(mismatched())),
            Err(error) => Err(streaming(error)),
        }
    }
}
