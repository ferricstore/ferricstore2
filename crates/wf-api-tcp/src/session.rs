//! FSNP v1 sessions over TCP (ADR-011).
//!
//! [`serve`] accepts connections and runs each one as a task. A connection
//! reads frames, dispatches each to the queue service without waiting for
//! it, and a writer task sends the replies. Replies on one lane leave in
//! request order; lanes are independent. The service calls back after the
//! decision is durable, from any thread.

mod connection;
mod deadline;
mod dispatch;
mod exec;
mod generic;
mod pipeline;
mod translate;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use wf_application::commands::flow::NodeService;
use wf_application::commands::queue::Cancellation;

/// What the handshake tells clients about this server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// Node name.
    pub node: String,
    /// Advertised host.
    pub host: String,
    /// Native protocol port.
    pub port: u16,
    /// Number of shards.
    pub shard_count: u16,
    /// Whether clients' `now_ms` reaches the node. Off in production: the
    /// node then decides due times and leases with its own clock, so a
    /// client cannot claim a future Flow early or take a live lease by
    /// sending a later time (ADR-015). Tests and parity probes turn it on.
    pub client_clock: bool,
    /// What one connection may have in flight (ADR-017).
    pub in_flight: InFlight,
    /// A replicated node's group members by id, for `SHARDS` (ADR-018);
    /// empty for a node that is not replicated.
    pub members: BTreeMap<u64, Endpoint>,
    /// How many connections the listener holds and how long a started
    /// frame may take (audit gaps G66, G67).
    pub connections: Connections,
    /// Whether the node serves a health port, as
    /// `FERRICSTORE.CAPABILITIES` tells (audit gap G73).
    pub health: bool,
}

/// The listener's guards, as Elixir's `maxclients` and
/// `native_frame_assembly_timeout_ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connections {
    /// Connections held at once; the listener takes the next one only when
    /// one closes, as Ranch does past `max_connections`.
    pub max: u32,
    /// How long a frame may take once its first byte arrived: the rest of
    /// its header, and its body once the connection admits it. A frame
    /// not finished in time closes the connection, since the stream
    /// cannot be resynced.
    pub frame_timeout: std::time::Duration,
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            max: 10_000,
            frame_timeout: std::time::Duration::from_secs(15),
        }
    }
}

/// Where clients reach one member (ADR-018).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Node name.
    pub node: String,
    /// Advertised host.
    pub host: String,
    /// Native protocol port.
    pub port: u16,
}

/// What one connection may have read and not yet answered before its reader
/// waits for replies to be written (ADR-017). A client that sends faster than
/// the node answers, or never reads its replies, is then held back by TCP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InFlight {
    /// Requests.
    pub requests: u32,
    /// Request body bytes; a larger request waits until nothing else on the
    /// connection is in flight.
    pub bytes: u32,
}

impl Default for InFlight {
    fn default() -> Self {
        Self {
            requests: 4_096,
            bytes: 64 * 1024 * 1024,
        }
    }
}

/// Accepts connections on `listener` until it fails; each connection runs
/// as its own task on the current Tokio runtime.
pub async fn serve(listener: TcpListener, service: Arc<dyn NodeService>, info: Arc<ServerInfo>) {
    let clients = AtomicU64::new(0);
    let slots = Arc::new(Semaphore::new(
        usize::try_from(info.connections.max.max(1)).unwrap_or(1),
    ));
    loop {
        // At the limit the listener waits for a connection to close before
        // it takes the next (audit gap G67).
        let Ok(slot) = Arc::clone(&slots).acquire_owned().await else {
            return;
        };
        let Ok((stream, _peer)) = listener.accept().await else {
            return;
        };
        // Small request and reply frames should leave at once.
        if stream.set_nodelay(true).is_err() {
            continue;
        }
        let client_id = clients.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        let context = dispatch::Context {
            service: Arc::clone(&service),
            info: Arc::clone(&info),
            client_id,
            closed: Cancellation::new(),
        };
        tokio::spawn(async move {
            connection::run(stream, context).await;
            drop(slot);
        });
    }
}
