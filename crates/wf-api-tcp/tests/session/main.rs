//! The FSNP listener (ADR-011) against a scripted queue service: handshake
//! fields the Python SDK requires, the SDK's queue frames with Elixir-shaped
//! replies, per-lane reply order and explicit refusals.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use wf_api_tcp::session::{ServerInfo, serve};
use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, QueueService, Reply,
    ServiceError,
};
use wf_domain::value::DomainError;

mod handshake;
mod options;
mod queue;
mod typed;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// Ids starting with `dup` already exist, `stale` completions have a stale
/// lease, and ids starting with `slow` reply after 150 ms from another thread.
/// A claim by worker `probe` echoes its options as the run state.
struct Scripted;

fn outcome(id: &[u8], refused: &[u8], error: DomainError) -> ItemOutcome {
    if id.starts_with(refused) {
        Err(ServiceError::Domain(error))
    } else {
        Ok(())
    }
}

impl QueueService for Scripted {
    fn create(&self, request: CreateFlows, reply: Reply<Vec<ItemOutcome>>) {
        let slow = request
            .flows
            .iter()
            .any(|flow| flow.id.starts_with(b"slow"));
        let outcomes = request
            .flows
            .iter()
            .map(|flow| outcome(&flow.id, b"dup", DomainError::AlreadyExists))
            .collect();
        if slow {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(150));
                reply(Ok(outcomes));
            });
        } else {
            reply(Ok(outcomes));
        }
    }

    fn claim(&self, request: ClaimFlows, reply: Reply<Vec<ClaimedFlow>>) {
        let job = ClaimedFlow {
            id: b"f1".to_vec(),
            partition: b"__flow_auto__:7".to_vec(),
            lease_token: request.worker.iter().chain(b":1:1").copied().collect(),
            fence: 1,
            run_state: if request.worker == b"probe" {
                format!("reclaim={:?}", request.reclaim).into_bytes()
            } else {
                b"queued".to_vec()
            },
        };
        reply(Ok(vec![job].into_iter().take(request.limit).collect()));
    }

    fn complete(&self, completions: Vec<CompleteFlow>, reply: Reply<Vec<ItemOutcome>>) {
        reply(Ok(completions
            .iter()
            .map(|completion| outcome(&completion.id, b"stale", DomainError::StaleLease))
            .collect()));
    }
}

/// A running listener on an ephemeral port; dropping it stops the runtime.
struct Running {
    _runtime: tokio::runtime::Runtime,
    address: SocketAddr,
}

fn start() -> TestResult<Running> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_io()
        .build()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
    let address = listener.local_addr()?;
    let info = ServerInfo {
        node: "test-node".into(),
        host: "127.0.0.1".into(),
        port: address.port(),
        shard_count: 4,
    };
    runtime.spawn(serve(listener, Arc::new(Scripted), Arc::new(info)));
    Ok(Running {
        _runtime: runtime,
        address,
    })
}

/// A blocking FSNP client.
struct Client {
    stream: TcpStream,
    next_id: u64,
}

/// A reply frame: header flags, lane, opcode, request id and body.
struct Frame {
    flags: u8,
    lane: u32,
    opcode: u16,
    request_id: u64,
    body: Vec<u8>,
}

impl Client {
    fn connect(running: &Running) -> TestResult<Self> {
        let stream = TcpStream::connect(running.address)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        Ok(Self { stream, next_id: 1 })
    }

    fn send(&mut self, opcode: u16, lane: u32, flags: u8, body: &[u8]) -> TestResult<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let mut frame = b"FSNP".to_vec();
        frame.extend_from_slice(&[0x01, flags]);
        frame.extend_from_slice(&lane.to_be_bytes());
        frame.extend_from_slice(&opcode.to_be_bytes());
        frame.extend_from_slice(&id.to_be_bytes());
        frame.extend_from_slice(&u32::try_from(body.len())?.to_be_bytes());
        frame.extend_from_slice(body);
        self.stream.write_all(&frame)?;
        Ok(id)
    }

    fn receive(&mut self) -> TestResult<Frame> {
        let mut header = [0_u8; 24];
        self.stream.read_exact(&mut header)?;
        assert_eq!(header.get(..5), Some(b"FSNP\x81".as_slice()));
        let field = |range: std::ops::Range<usize>| header.get(range).ok_or("short header");
        let mut body = vec![0; usize::try_from(u32::from_be_bytes(field(20..24)?.try_into()?))?];
        self.stream.read_exact(&mut body)?;
        Ok(Frame {
            flags: *header.get(5).ok_or("short header")?,
            lane: u32::from_be_bytes(field(6..10)?.try_into()?),
            opcode: u16::from_be_bytes(field(10..12)?.try_into()?),
            request_id: u64::from_be_bytes(field(12..20)?.try_into()?),
            body,
        })
    }

    /// Sends one request and returns its reply.
    fn call(&mut self, opcode: u16, lane: u32, flags: u8, body: &[u8]) -> TestResult<Frame> {
        let id = self.send(opcode, lane, flags, body)?;
        let reply = self.receive()?;
        assert_eq!(
            (reply.request_id, reply.opcode, reply.lane),
            (id, opcode, lane)
        );
        Ok(reply)
    }
}
