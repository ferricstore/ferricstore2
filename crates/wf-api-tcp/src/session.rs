//! FSNP v1 sessions over TCP (ADR-011).
//!
//! [`serve`] accepts connections and runs each one as a task. A connection
//! reads frames, dispatches each to the queue service without waiting for
//! it, and a writer task sends the replies. Replies on one lane leave in
//! request order; lanes are independent. The service calls back after the
//! decision is durable, from any thread.

mod connection;
mod dispatch;
mod generic;
mod translate;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::net::TcpListener;
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
}

/// Accepts connections on `listener` until it fails; each connection runs
/// as its own task on the current Tokio runtime.
pub async fn serve(listener: TcpListener, service: Arc<dyn NodeService>, info: Arc<ServerInfo>) {
    let clients = AtomicU64::new(0);
    while let Ok((stream, _peer)) = listener.accept().await {
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
        tokio::spawn(connection::run(stream, context));
    }
}
