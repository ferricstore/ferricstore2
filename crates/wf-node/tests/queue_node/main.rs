//! The queue node (ADR-011): shard threads decide each record with the domain
//! reducer, log every change to the group-committed log, release replies only
//! after the sync, and rebuild their state from the log on restart.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, NewFlow, QueueService, Reply,
    ServiceError,
};
use wf_node::composition::{QueueNode, QueueNodeConfig};

mod audit;
mod backpressure;
mod blocking;
mod cold;
mod crash;
mod flow_approval;
mod flow_budget;
mod flow_children;
mod flow_children_close;
mod flow_circuit;
mod flow_claim;
mod flow_cleanup;
mod flow_effect;
mod flow_fifo;
mod flow_governed_claim;
mod flow_info;
mod flow_limit;
mod flow_mutate;
mod flow_overview;
mod flow_payload;
mod flow_policy;
mod flow_policy_governance;
mod flow_query;
mod flow_query_order;
mod flow_record;
mod flow_rewind;
mod flow_schedule;
mod flow_signal;
mod flow_steps;
mod flow_text;
mod flow_text_batches;
mod flow_value;
mod kept_on_disk;
mod lifecycle;
mod log_growth;
mod reclaim;
mod replica;
mod replica_frames;
mod replica_lease;
mod replica_reads;
mod replica_roles;
mod replica_trim;
mod restart;
mod retention;
mod selection;
mod snapshot;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

fn directory(tag: &str) -> TestResult<PathBuf> {
    let suffix = NEXT_PATH.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("wf-queue-node-{}-{suffix}-{tag}", unique_process()));
    std::fs::create_dir(&path)?;
    Ok(path)
}

/// The process id and the time: ids repeat, and nextest runs each test in a
/// process of its own, so a reused id met an earlier run's directory.
fn unique_process() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

fn open(path: &Path, shards: u16) -> TestResult<QueueNode> {
    open_with(
        path,
        QueueNodeConfig {
            shards,
            ..QueueNodeConfig::default()
        },
    )
}

fn open_with(path: &Path, config: QueueNodeConfig) -> TestResult<QueueNode> {
    Ok(QueueNode::open(path, config)?)
}

/// Calls a service method and waits for its single reply.
fn call<T: Send + 'static>(start: impl FnOnce(Reply<T>)) -> TestResult<Result<T, ServiceError>> {
    let (sender, receiver) = mpsc::channel();
    start(Box::new(move |result| {
        // The test may have timed out and dropped the receiver.
        let _delivered = sender.send(result);
    }));
    Ok(receiver.recv_timeout(Duration::from_secs(20))?)
}

fn flow(id: &str) -> NewFlow {
    NewFlow {
        id: id.as_bytes().to_vec(),
        partition: None,
        payload: Some(Vec::new()),
    }
}

fn create(node: &QueueNode, ids: &[&str]) -> TestResult<Vec<ItemOutcome>> {
    let request = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: None,
        priority: 0,
        flows: ids.iter().map(|id| flow(id)).collect(),
    };
    Ok(call(|reply| node.create(request, reply))??)
}

fn claim_request(limit: usize) -> ClaimFlows {
    ClaimFlows {
        flow_type: b"jobs".to_vec(),
        states: vec![b"queued".to_vec()],
        worker: b"w1".to_vec(),
        lease_ms: 30_000,
        limit,
        priority: None,
        partitions: None,
        any_partition: false,
        reclaim: None,
        block_ms: None,
        cancellation: None,
        now_ms: None,
        records: None,
        governance: wf_application::commands::queue::ClaimGovernance::default(),
    }
}

fn claim(node: &QueueNode, limit: usize) -> TestResult<Vec<ClaimedFlow>> {
    Ok(call(|reply| node.claim(claim_request(limit), reply))??)
}

fn completion(job: &ClaimedFlow) -> CompleteFlow {
    CompleteFlow {
        id: job.id.clone(),
        partition: Some(job.partition.clone()),
        lease_token: job.lease_token.clone(),
        fence: job.fence,
        result: None,
    }
}

fn complete(node: &QueueNode, completions: Vec<CompleteFlow>) -> TestResult<Vec<ItemOutcome>> {
    Ok(call(|reply| node.complete(completions, reply))??)
}
