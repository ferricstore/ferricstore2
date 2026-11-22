//! Starting the threads both node modes share (ADR-011, ADR-018): the
//! waiter thread, one thread per shard and the router over them.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread::JoinHandle;

use crate::supervision;
use crate::supervision::shard::role::Role;
use crate::supervision::shard::sink::Sink;
use crate::supervision::shard::state::ShardState;
use crate::supervision::shard::{self, Links};
use crate::supervision::waiters::Waiters;

use super::router::{Router, ShardLink};
use super::{NodeError, QueueNodeConfig, credit};

/// The threads and router both node modes start: the waiter thread, one
/// thread per shard sending its rounds to `sink`, and the router.
pub(crate) struct Started {
    pub(crate) router: Arc<Router>,
    pub(crate) waiters: Waiters,
    pub(crate) waiter: JoinHandle<()>,
    pub(crate) shards: Vec<JoinHandle<()>>,
}

impl Started {
    pub(crate) fn new(
        states: Vec<ShardState>,
        config: &QueueNodeConfig,
        sink: Sink,
        role: Role,
    ) -> Result<Self, NodeError> {
        let (waiter_sender, waiter_receiver) = mpsc::channel();
        let waiters = Waiters::new(waiter_sender);
        let waiter_handle = waiters.clone();
        let waiter = supervision::spawn("wf-waiters".into(), move || {
            waiter_handle.run(&waiter_receiver);
        })
        .ok_or(NodeError::Thread)?;
        let release_router = Arc::new(OnceLock::new());
        let running = credit::running::opened(states.first());
        let poisoned = match &sink {
            Sink::Group { poisoned, .. } => Arc::clone(poisoned),
            Sink::Local { .. } => Arc::new(AtomicBool::new(false)),
        };
        let (links, shards) = start_shards(
            states,
            &Links {
                index: 0,
                sink,
                role: role.clone(),
                ready: Arc::new(AtomicU64::new(0)),
                leased: Arc::new(AtomicU64::new(0)),
                expiry: Arc::new(AtomicU64::new(u64::MAX)),
                by_priority: Arc::new([AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)]),
                waiters: waiters.clone(),
                release: credit::releaser(Arc::clone(&release_router)),
                backlog: Arc::new(AtomicUsize::new(0)),
            },
        )?;
        let router = Arc::new(Router {
            shards: links,
            shard_count: config.shards,
            waiters: waiters.clone(),
            running,
            admission: config.admission,
            role,
            poisoned,
            claim_turn: std::sync::atomic::AtomicUsize::new(0),
        });
        let _set = release_router.set(Arc::downgrade(&router));
        Ok(Self {
            router,
            waiters,
            waiter,
            shards,
        })
    }
}

/// Starts one thread per shard; each gets `shared`'s log, completions,
/// waiters, append size and releases, and its own index and counters.
fn start_shards(
    states: Vec<ShardState>,
    shared: &Links,
) -> Result<(Vec<ShardLink>, Vec<JoinHandle<()>>), NodeError> {
    let mut links = Vec::with_capacity(states.len());
    let mut threads = Vec::with_capacity(states.len());
    for (index, state) in states.into_iter().enumerate() {
        let (sender, receiver) = mpsc::channel();
        let ready = Arc::new(AtomicU64::new(state.ready()));
        let leased = Arc::new(AtomicU64::new(state.leased()));
        let expiry = Arc::new(AtomicU64::new(state.earliest_lease()));
        let by_priority = Arc::new(state.ready_by_priority().map(AtomicU64::new));
        let backlog = Arc::new(AtomicUsize::new(0));
        let shard_links = Links {
            index: u16::try_from(index).map_err(|_| NodeError::InvalidShards)?,
            sink: shared.sink.clone(),
            role: shared.role.clone(),
            ready: Arc::clone(&ready),
            leased: Arc::clone(&leased),
            expiry: Arc::clone(&expiry),
            by_priority: Arc::clone(&by_priority),
            waiters: shared.waiters.clone(),
            release: Arc::clone(&shared.release),
            backlog: Arc::clone(&backlog),
        };
        threads.push(
            supervision::spawn(format!("wf-shard-{index}"), move || {
                shard::run(state, &receiver, &shard_links);
            })
            .ok_or(NodeError::Thread)?,
        );
        links.push(ShardLink {
            requests: sender,
            ready,
            leased,
            expiry,
            by_priority,
            backlog,
        });
    }
    Ok((links, threads))
}
