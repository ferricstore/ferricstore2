//! The queue node binary (ADR-011): opens the node in a data directory and
//! serves the FSNP queue commands on one TCP port.
//!
//! Usage: `ferricstore2 --data-dir PATH [--host 127.0.0.1] [--port 6388]
//! [--shards 16] [--threads N] [--retention-ms 604800000]
//! [--client-clock false] [--max-new-backlog 16384] [--max-backlog 65536]
//! [--node-id ID --member ID=HOST:PORT/PEER_HOST:PEER_PORT ...]`. With
//! `--node-id` the node is that member of a replicated group (ADR-018):
//! `--member` names every member's client and Raft addresses, this one's
//! included, and the node exits (status 75) once a lost round stops it, for
//! its supervisor to restart it. `--snapshot-every N` (100000) entries
//! applied, a follower snapshots; `--keep-logs N` (1000) stay behind it.
//! With `--client-clock true` clients' `now_ms` decides due times and leases
//! (tests and parity probes only; ADR-015). The backlog flags are the
//! per-shard admission limits in items (ADR-017). The data
//! directory holds `log/` and `snapshots/`. Port 0 picks a free port. Once
//! listening it prints `READY port=<port> shards=<n>`.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use wf_api_tcp::session::{Endpoint, InFlight, ServerInfo, serve};
use wf_application::commands::flow::NodeService;
use wf_node::composition::{Admission, QueueNode, QueueNodeConfig, Replica};

/// The node allocates on every request; mimalloc's per-thread free lists
/// cost far less than the system allocator (ADR-015).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

struct Options {
    data_dir: PathBuf,
    host: String,
    port: u16,
    shards: u16,
    threads: Option<usize>,
    retention_ms: u64,
    client_clock: bool,
    admission: Admission,
    node_id: Option<u64>,
    snapshot_every: u64,
    keep_logs: u64,
    /// Members by id: client host and port, and Raft address.
    members: BTreeMap<u64, (String, u16, SocketAddr)>,
}

/// One `--member ID=HOST:PORT/PEER_HOST:PEER_PORT`.
fn member(value: &str) -> Result<(u64, (String, u16, SocketAddr)), String> {
    let invalid = || format!("--member {value} needs ID=HOST:PORT/PEER_HOST:PEER_PORT");
    let (id, addresses) = value.split_once('=').ok_or_else(invalid)?;
    let (client, peer) = addresses.split_once('/').ok_or_else(invalid)?;
    let (host, port) = client.rsplit_once(':').ok_or_else(invalid)?;
    let peer = peer
        .to_socket_addrs()
        .ok()
        .and_then(|mut found| found.next())
        .ok_or_else(invalid)?;
    Ok((
        id.parse().map_err(|_| invalid())?,
        (host.to_owned(), port.parse().map_err(|_| invalid())?, peer),
    ))
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        data_dir: PathBuf::new(),
        host: "127.0.0.1".into(),
        port: 6388,
        shards: QueueNodeConfig::default().shards,
        threads: None,
        retention_ms: QueueNodeConfig::default().retention_ms,
        client_clock: false,
        admission: Admission::default(),
        node_id: None,
        snapshot_every: 100_000,
        keep_logs: 1_000,
        members: BTreeMap::new(),
    };
    let mut pairs = arguments.iter();
    while let Some(flag) = pairs.next() {
        let value = pairs
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        let number = || {
            value
                .parse::<u64>()
                .map_err(|_| format!("{flag} needs a number"))
        };
        match flag.as_str() {
            "--data-dir" => options.data_dir = PathBuf::from(value),
            "--host" => options.host.clone_from(value),
            "--port" => {
                options.port =
                    u16::try_from(number()?).map_err(|_| "--port is too large".to_owned())?;
            }
            "--shards" => {
                options.shards =
                    u16::try_from(number()?).map_err(|_| "--shards is too large".to_owned())?;
            }
            "--threads" => {
                options.threads = Some(
                    usize::try_from(number()?).map_err(|_| "--threads is too large".to_owned())?,
                );
            }
            "--retention-ms" => options.retention_ms = number()?,
            "--client-clock" => {
                options.client_clock = match value.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err("--client-clock needs true or false".into()),
                };
            }
            "--max-new-backlog" => {
                options.admission.new_work = usize::try_from(number()?)
                    .map_err(|_| "--max-new-backlog is too large".to_owned())?;
            }
            "--max-backlog" => {
                options.admission.other = usize::try_from(number()?)
                    .map_err(|_| "--max-backlog is too large".to_owned())?;
            }
            "--node-id" => options.node_id = Some(number()?),
            "--snapshot-every" => options.snapshot_every = number()?,
            "--keep-logs" => options.keep_logs = number()?,
            "--member" => {
                let (id, addresses) = member(value)?;
                options.members.insert(id, addresses);
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if options.data_dir.as_os_str().is_empty() {
        return Err("--data-dir is required".into());
    }
    Ok(options)
}

fn run(options: Options) -> Result<(), String> {
    let config = QueueNodeConfig {
        shards: options.shards,
        retention_ms: options.retention_ms,
        admission: options.admission,
        ..QueueNodeConfig::default()
    };
    let node = open(&options, config)?;
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    if let Some(threads) = options.threads {
        builder.worker_threads(threads);
    }
    let runtime = builder
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| error.to_string())?;
    let address = format!("{}:{}", options.host, options.port);
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&address)
            .await
            .map_err(|error| error.to_string())?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        let members: BTreeMap<u64, Endpoint> = options
            .members
            .iter()
            .map(|(id, (host, port, _))| {
                let endpoint = Endpoint {
                    node: format!("ferricstore2-{id}"),
                    host: host.clone(),
                    port: *port,
                };
                (*id, endpoint)
            })
            .collect();
        let name = options.node_id.map_or_else(
            || "ferricstore2".to_owned(),
            |id| format!("ferricstore2-{id}"),
        );
        let info = ServerInfo {
            node: name,
            host: options.host,
            port,
            shard_count: options.shards,
            client_clock: options.client_clock,
            in_flight: InFlight::default(),
            members,
        };
        let node = Arc::new(node);
        let mut stdout = std::io::stdout();
        write!(stdout, "READY port={port} shards={}", options.shards)
            .and_then(|()| stdout.write_all(b"\n"))
            .and_then(|()| stdout.flush())
            .map_err(|error| error.to_string())?;
        let service: Arc<dyn NodeService> = Arc::clone(&node) as Arc<dyn NodeService>;
        let server = tokio::spawn(serve(listener, service, Arc::new(info)));
        poisoned(node).await;
        server.abort();
        Err(LOST.to_owned())
    })
}

/// Opens the node: a member of its group with `--node-id`, else on its own.
fn open(options: &Options, config: QueueNodeConfig) -> Result<QueueNode, String> {
    let Some(id) = options.node_id else {
        return QueueNode::open(&options.data_dir, config).map_err(|error| error.to_string());
    };
    let (_, _, peer) = options
        .members
        .get(&id)
        .ok_or_else(|| format!("--node-id {id} is not among the --member flags"))?;
    let listener = std::net::TcpListener::bind(peer).map_err(|error| error.to_string())?;
    let replica = Replica {
        id,
        peers: options
            .members
            .iter()
            .map(|(id, (_, _, peer))| (*id, *peer))
            .collect(),
        listener,
        snapshot_every: options.snapshot_every,
        keep_logs: options.keep_logs,
    };
    QueueNode::open_replicated(&options.data_dir, config, replica)
        .map_err(|error| error.to_string())
}

/// Returns once a lost round stopped the node (fail-stop, ADR-018).
async fn poisoned(node: Arc<QueueNode>) {
    while !node.poisoned() {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// The exit status of a node a lost round stopped, for its supervisor to
/// restart it; it then rebuilds from the group's log.
const POISONED: u8 = 75;

/// The failure of a node a lost round stopped.
const LOST: &str = "a replicated round was lost; restart the node";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match parse(&arguments).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let status = if message == LOST {
                ExitCode::from(POISONED)
            } else {
                ExitCode::FAILURE
            };
            let mut stderr = std::io::stderr();
            // Nothing else can report a failure to write the failure.
            let _reported =
                write!(stderr, "ferricstore2: {message}").and_then(|()| stderr.write_all(b"\n"));
            status
        }
    }
}
