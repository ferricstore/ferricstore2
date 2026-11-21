//! The queue node binary (ADR-011): opens the node in a data directory and
//! serves the FSNP queue commands on one TCP port.
//!
//! Every setting is a `FERRICSTORE_*` environment variable, and each has a
//! flag that overrides it (ADR-019):
//!
//! | Flag | Variable | Default |
//! |---|---|---|
//! | `--data-dir` | `FERRICSTORE_DATA_DIR` | required |
//! | `--host` | `FERRICSTORE_BIND_HOST` | `127.0.0.1` |
//! | `--port` | `FERRICSTORE_NATIVE_PORT` | `6388` (0 picks one) |
//! | `--advertise-host` | `FERRICSTORE_ADVERTISE_HOST` | `FERRICSTORE_NODE_NAME` (`--node-name`), `HOSTNAME`, then the bind host |
//! | `--peer-port` | `FERRICSTORE_PEER_PORT` | `7388` |
//! | `--health-port` | `FERRICSTORE_HEALTH_PORT` | off |
//! | `--shards` | `FERRICSTORE_SHARDS` | `16` |
//! | `--threads` | `FERRICSTORE_THREADS` | Tokio's |
//! | `--retention-ms` | `FERRICSTORE_RETENTION_MS` | 7 days |
//! | `--client-clock` | `FERRICSTORE_CLIENT_CLOCK` | `false` (tests and parity probes only; ADR-015) |
//! | `--max-new-backlog`, `--max-backlog` | `FERRICSTORE_MAX_NEW_BACKLOG`, `FERRICSTORE_MAX_BACKLOG` | ADR-017 |
//! | `--discovery` | `FERRICSTORE_DISCOVERY` | `none`; `static`, `dns` (see `configuration`) |
//! | `--cluster-nodes` | `FERRICSTORE_CLUSTER_NODES` | for `static` |
//! | `--node-id` | `FERRICSTORE_NODE_ID` | from the advertised host |
//! | `--dns-name`, `--cluster-size` | `FERRICSTORE_DNS_NAME`, `FERRICSTORE_CLUSTER_SIZE` | for `dns` |
//! | `--snapshot-every`, `--keep-logs` | `FERRICSTORE_SNAPSHOT_EVERY`, `FERRICSTORE_KEEP_LOGS` | `100000`, `1000` |
//!
//! A replicated member exits with status 75 once a lost round stops it, for
//! its supervisor to restart it (ADR-018). Once listening it prints `READY
//! port=<port> shards=<n>`.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use wf_api_tcp::session::{Endpoint, InFlight, ServerInfo, serve};
use wf_application::commands::flow::NodeService;
use wf_node::composition::{Admission, QueueNode, QueueNodeConfig, Replica};
use wf_node::configuration::{self, Cluster, Local};
use wf_node::supervision::health;

/// The node allocates on every request; mimalloc's per-thread free lists
/// cost far less than the system allocator (ADR-015).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Each flag and the variable it overrides.
const FLAGS: [(&str, &str); 20] = [
    ("--data-dir", "FERRICSTORE_DATA_DIR"),
    ("--host", "FERRICSTORE_BIND_HOST"),
    ("--port", "FERRICSTORE_NATIVE_PORT"),
    ("--advertise-host", "FERRICSTORE_ADVERTISE_HOST"),
    ("--peer-port", "FERRICSTORE_PEER_PORT"),
    ("--health-port", "FERRICSTORE_HEALTH_PORT"),
    ("--shards", "FERRICSTORE_SHARDS"),
    ("--threads", "FERRICSTORE_THREADS"),
    ("--retention-ms", "FERRICSTORE_RETENTION_MS"),
    ("--client-clock", "FERRICSTORE_CLIENT_CLOCK"),
    ("--max-new-backlog", "FERRICSTORE_MAX_NEW_BACKLOG"),
    ("--max-backlog", "FERRICSTORE_MAX_BACKLOG"),
    ("--discovery", "FERRICSTORE_DISCOVERY"),
    ("--cluster-nodes", "FERRICSTORE_CLUSTER_NODES"),
    ("--node-id", "FERRICSTORE_NODE_ID"),
    ("--dns-name", "FERRICSTORE_DNS_NAME"),
    ("--cluster-size", "FERRICSTORE_CLUSTER_SIZE"),
    ("--snapshot-every", "FERRICSTORE_SNAPSHOT_EVERY"),
    ("--keep-logs", "FERRICSTORE_KEEP_LOGS"),
    ("--node-name", "FERRICSTORE_NODE_NAME"),
];

/// The environment's settings, then the flags over them.
fn settings(arguments: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut settings: BTreeMap<String, String> = std::env::vars()
        .filter(|(name, _)| name.starts_with("FERRICSTORE_") || name == "HOSTNAME")
        .collect();
    let mut pairs = arguments.iter();
    while let Some(flag) = pairs.next() {
        let value = pairs
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        let (_, name) = FLAGS
            .iter()
            .find(|(known, _)| *known == flag)
            .ok_or_else(|| format!("unknown flag {flag}"))?;
        settings.insert((*name).to_owned(), value.clone());
    }
    Ok(settings)
}

struct Options {
    data_dir: PathBuf,
    bind_host: String,
    advertise_host: String,
    port: u16,
    peer_port: u16,
    health_port: Option<u16>,
    threads: Option<usize>,
    client_clock: bool,
    config: QueueNodeConfig,
    snapshot_every: u64,
    keep_logs: u64,
    cluster: Option<Cluster>,
}

fn number<T: std::str::FromStr>(
    settings: &BTreeMap<String, String>,
    name: &str,
) -> Result<Option<T>, String> {
    settings
        .get(name)
        .map(|value| {
            value
                .trim()
                .parse()
                .map_err(|_| format!("{name} needs a number, not {value:?}"))
        })
        .transpose()
}

fn options(settings: &BTreeMap<String, String>) -> Result<Options, String> {
    let text = |name: &str| {
        settings
            .get(name)
            .filter(|value| !value.is_empty())
            .cloned()
    };
    let data_dir = text("FERRICSTORE_DATA_DIR").ok_or("--data-dir is required")?;
    let bind_host = text("FERRICSTORE_BIND_HOST").unwrap_or_else(|| "127.0.0.1".into());
    let advertise_host = text("FERRICSTORE_ADVERTISE_HOST")
        .or_else(|| text("FERRICSTORE_NODE_NAME"))
        .or_else(|| text("HOSTNAME"))
        .unwrap_or_else(|| bind_host.clone());
    let port = number(settings, "FERRICSTORE_NATIVE_PORT")?.unwrap_or(6388);
    let peer_port = number(settings, "FERRICSTORE_PEER_PORT")?.unwrap_or(7388);
    let client_clock = match text("FERRICSTORE_CLIENT_CLOCK").as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        Some(other) => return Err(format!("--client-clock needs true or false, not {other}")),
    };
    let defaults = QueueNodeConfig::default();
    let config = QueueNodeConfig {
        shards: number(settings, "FERRICSTORE_SHARDS")?.unwrap_or(defaults.shards),
        retention_ms: number(settings, "FERRICSTORE_RETENTION_MS")?
            .unwrap_or(defaults.retention_ms),
        admission: Admission {
            new_work: number(settings, "FERRICSTORE_MAX_NEW_BACKLOG")?
                .unwrap_or(defaults.admission.new_work),
            other: number(settings, "FERRICSTORE_MAX_BACKLOG")?.unwrap_or(defaults.admission.other),
        },
        ..defaults
    };
    let local = Local {
        host: advertise_host.clone(),
        port,
        peer_port,
    };
    let lookup = |name: &str| settings.get(name).cloned();
    let cluster = configuration::cluster(&lookup, &local)?;
    Ok(Options {
        data_dir: PathBuf::from(data_dir),
        bind_host,
        advertise_host,
        port,
        peer_port,
        health_port: number(settings, "FERRICSTORE_HEALTH_PORT")?,
        threads: number(settings, "FERRICSTORE_THREADS")?,
        client_clock,
        config,
        snapshot_every: number(settings, "FERRICSTORE_SNAPSHOT_EVERY")?.unwrap_or(100_000),
        keep_logs: number(settings, "FERRICSTORE_KEEP_LOGS")?.unwrap_or(1_000),
        cluster,
    })
}

/// The client endpoints of a cluster's members, for `SHARDS`.
fn endpoints(cluster: Option<&Cluster>) -> BTreeMap<u64, Endpoint> {
    cluster
        .map(|cluster| {
            cluster
                .members
                .iter()
                .map(|(id, member)| {
                    let endpoint = Endpoint {
                        node: format!("ferricstore2-{id}"),
                        host: member.host.clone(),
                        port: member.port,
                    };
                    (*id, endpoint)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn run(options: Options) -> Result<(), String> {
    let node = Arc::new(open(&options)?);
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    if let Some(threads) = options.threads {
        builder.worker_threads(threads);
    }
    let runtime = builder
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let address = format!("{}:{}", options.bind_host, options.port);
        let listener = tokio::net::TcpListener::bind(&address)
            .await
            .map_err(|error| format!("{address}: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        if let Some(health_port) = options.health_port {
            let address = format!("{}:{health_port}", options.bind_host);
            let health = tokio::net::TcpListener::bind(&address)
                .await
                .map_err(|error| format!("{address}: {error}"))?;
            let probed = Arc::clone(&node);
            tokio::spawn(health::serve(health, Arc::new(move || probed.ready())));
        }
        let info = ServerInfo {
            node: options.cluster.as_ref().map_or_else(
                || "ferricstore2".to_owned(),
                |cluster| format!("ferricstore2-{}", cluster.id),
            ),
            host: options.advertise_host.clone(),
            port,
            shard_count: options.config.shards,
            client_clock: options.client_clock,
            in_flight: InFlight::default(),
            members: endpoints(options.cluster.as_ref()),
        };
        let mut stdout = std::io::stdout();
        write!(stdout, "READY port={port} shards={}", options.config.shards)
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

/// Opens the node: a member of its cluster, or on its own.
fn open(options: &Options) -> Result<QueueNode, String> {
    let Some(cluster) = &options.cluster else {
        return QueueNode::open(&options.data_dir, options.config)
            .map_err(|error| error.to_string());
    };
    let address = format!("{}:{}", options.bind_host, options.peer_port);
    let listener =
        std::net::TcpListener::bind(&address).map_err(|error| format!("{address}: {error}"))?;
    let replica = Replica {
        id: cluster.id,
        peers: cluster
            .members
            .iter()
            .map(|(id, member)| (*id, member.peer()))
            .collect(),
        listener,
        snapshot_every: options.snapshot_every,
        keep_logs: options.keep_logs,
    };
    QueueNode::open_replicated(&options.data_dir, options.config, replica)
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
    match settings(&arguments)
        .and_then(|settings| options(&settings))
        .and_then(run)
    {
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
