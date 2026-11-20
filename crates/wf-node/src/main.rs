//! The queue node binary (ADR-011): opens the node in a data directory and
//! serves the FSNP queue commands on one TCP port.
//!
//! Usage: `ferricstore2 --data-dir PATH [--host 127.0.0.1] [--port 6388]
//! [--shards 16] [--threads N] [--retention-ms 604800000]`. The data
//! directory holds `log/` and `snapshots/`. Port 0 picks a free port. Once
//! listening it prints `READY port=<port> shards=<n>`.

#![forbid(unsafe_code)]

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use wf_api_tcp::session::{ServerInfo, serve};
use wf_node::composition::{QueueNode, QueueNodeConfig};

struct Options {
    data_dir: PathBuf,
    host: String,
    port: u16,
    shards: u16,
    threads: Option<usize>,
    retention_ms: u64,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        data_dir: PathBuf::new(),
        host: "127.0.0.1".into(),
        port: 6388,
        shards: QueueNodeConfig::default().shards,
        threads: None,
        retention_ms: QueueNodeConfig::default().retention_ms,
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
        ..QueueNodeConfig::default()
    };
    let node = QueueNode::open(&options.data_dir, config).map_err(|error| error.to_string())?;
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    if let Some(threads) = options.threads {
        builder.worker_threads(threads);
    }
    let runtime = builder
        .enable_io()
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
        let info = ServerInfo {
            node: "ferricstore2".into(),
            host: options.host,
            port,
            shard_count: options.shards,
        };
        let mut stdout = std::io::stdout();
        write!(stdout, "READY port={port} shards={}", options.shards)
            .and_then(|()| stdout.write_all(b"\n"))
            .and_then(|()| stdout.flush())
            .map_err(|error| error.to_string())?;
        serve(listener, Arc::new(node), Arc::new(info)).await;
        Ok(())
    })
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match parse(&arguments).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let mut stderr = std::io::stderr();
            // Nothing else can report a failure to write the failure.
            let _reported =
                write!(stderr, "ferricstore2: {message}").and_then(|()| stderr.write_all(b"\n"));
            ExitCode::FAILURE
        }
    }
}
