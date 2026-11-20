//! The waiter thread (ADR-013): blocking claims that found nothing park here
//! until a create of a matching Flow, their earliest due time, their deadline,
//! cancellation or shutdown.
//!
//! Shards bump a node-wide epoch after a round that created Flows, and only
//! then check whether anything is parked. A claim reads the epoch before its
//! attempt; parking counts the claim first, then compares the epoch and
//! retries at once if it moved. So a create that lands between an empty
//! attempt and parking always wakes the claim.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use wf_application::commands::queue::Cancellation;

/// Most claims parked at once; one more is refused.
const MAX_PARKED: usize = 65_536;
/// How often parked claims are checked for cancellation.
const CANCEL_CHECK: Duration = Duration::from_millis(100);

/// What a parked claim does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wake {
    /// Try again: a matching Flow may be claimable.
    Retry,
    /// The deadline passed.
    Expired,
    /// The requester is gone.
    Cancelled,
    /// The node is stopping.
    Stopped,
    /// Too many claims are parked.
    Full,
}

/// A blocking claim that found nothing.
pub(crate) struct Parked {
    pub(crate) flow_type: Vec<u8>,
    /// Required state; any state when absent.
    pub(crate) state: Option<Vec<u8>>,
    /// Required priority; any priority when absent.
    pub(crate) priority: Option<u8>,
    /// The epoch read before the attempt that found nothing.
    pub(crate) epoch: u64,
    /// When to give up; never when absent.
    pub(crate) deadline: Option<Instant>,
    /// The earliest time a matching Flow falls due, if known.
    pub(crate) due: Option<Instant>,
    pub(crate) cancellation: Option<Cancellation>,
    pub(crate) resume: Box<dyn FnOnce(Wake) + Send>,
}

impl Parked {
    fn timer(&self) -> Option<Instant> {
        match (self.deadline, self.due) {
            (Some(deadline), Some(due)) => Some(deadline.min(due)),
            (deadline, due) => deadline.or(due),
        }
    }

    fn matches(&self, arrival: &Arrival) -> bool {
        self.flow_type == arrival.flow_type
            && self
                .state
                .as_ref()
                .is_none_or(|state| *state == arrival.state)
            && self
                .priority
                .is_none_or(|priority| priority == arrival.priority)
    }
}

/// Created Flows of one queue in one shard round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Arrival {
    pub(crate) flow_type: Vec<u8>,
    pub(crate) state: Vec<u8>,
    pub(crate) priority: u8,
}

/// Work for the waiter thread.
pub(crate) enum Message {
    Park(Parked),
    Arrived(Vec<Arrival>),
    Stop,
}

/// The node's handle on the waiter thread, shared by shards and claims.
#[derive(Clone)]
pub(crate) struct Waiters {
    sender: Sender<Message>,
    parked: Arc<AtomicUsize>,
    epoch: Arc<AtomicU64>,
}

impl Waiters {
    pub(crate) fn new(sender: Sender<Message>) -> Self {
        Self {
            sender,
            parked: Arc::new(AtomicUsize::new(0)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The epoch a claim reads before its attempt.
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Parks a claim; a stopped thread resumes it as stopped.
    pub(crate) fn park(&self, parked: Parked) {
        if let Err(returned) = self.sender.send(Message::Park(parked))
            && let Message::Park(parked) = returned.0
        {
            (parked.resume)(Wake::Stopped);
        }
    }

    /// Called by a shard after a round that created Flows; `arrivals` runs
    /// only when a claim is parked.
    pub(crate) fn arrived(&self, arrivals: impl FnOnce() -> Vec<Arrival>) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        if self.parked.load(Ordering::SeqCst) > 0 {
            // Only a stopped node has no waiter thread.
            let _stopped = self.sender.send(Message::Arrived(arrivals()));
        }
    }

    /// Asks the thread to resume every parked claim as stopped and exit.
    pub(crate) fn stop(&self) {
        // The thread may already be gone.
        let _stopped = self.sender.send(Message::Stop);
    }

    /// Runs the thread until [`Self::stop`].
    pub(crate) fn run(&self, messages: &Receiver<Message>) {
        let mut checked = Instant::now();
        let mut board = Board {
            next: 0,
            parked: HashMap::new(),
            by_type: HashMap::new(),
            timers: BTreeSet::new(),
            count: Arc::clone(&self.parked),
            epoch: Arc::clone(&self.epoch),
        };
        loop {
            let now = Instant::now();
            board.fire(now);
            let timeout = board
                .timers
                .first()
                .map_or(CANCEL_CHECK, |(at, _)| at.saturating_duration_since(now))
                .min(CANCEL_CHECK);
            match messages.recv_timeout(timeout) {
                Ok(Message::Park(parked)) => board.park(parked),
                Ok(Message::Arrived(arrivals)) => board.arrived(&arrivals),
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if checked.elapsed() >= CANCEL_CHECK {
                board.cancelled();
                checked = Instant::now();
            }
        }
        let ids: Vec<u64> = board.parked.keys().copied().collect();
        for id in ids {
            board.resume(id, Wake::Stopped);
        }
        while let Ok(message) = messages.try_recv() {
            if let Message::Park(parked) = message {
                (parked.resume)(Wake::Stopped);
            }
        }
    }
}

struct Board {
    next: u64,
    parked: HashMap<u64, Parked>,
    by_type: HashMap<Vec<u8>, Vec<u64>>,
    timers: BTreeSet<(Instant, u64)>,
    count: Arc<AtomicUsize>,
    epoch: Arc<AtomicU64>,
}

impl Board {
    fn park(&mut self, parked: Parked) {
        self.count.fetch_add(1, Ordering::SeqCst);
        let moved = self.epoch.load(Ordering::SeqCst) != parked.epoch;
        let wake = if moved {
            Some(Wake::Retry)
        } else if self.parked.len() >= MAX_PARKED {
            Some(Wake::Full)
        } else {
            None
        };
        if let Some(wake) = wake {
            self.count.fetch_sub(1, Ordering::SeqCst);
            (parked.resume)(wake);
            return;
        }
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        if let Some(at) = parked.timer() {
            self.timers.insert((at, id));
        }
        self.by_type
            .entry(parked.flow_type.clone())
            .or_default()
            .push(id);
        self.parked.insert(id, parked);
    }

    fn resume(&mut self, id: u64, wake: Wake) {
        let Some(parked) = self.parked.remove(&id) else {
            return;
        };
        self.count.fetch_sub(1, Ordering::SeqCst);
        if let Some(at) = parked.timer() {
            self.timers.remove(&(at, id));
        }
        if let Some(ids) = self.by_type.get_mut(&parked.flow_type) {
            ids.retain(|other| *other != id);
            if ids.is_empty() {
                self.by_type.remove(&parked.flow_type);
            }
        }
        (parked.resume)(wake);
    }

    fn arrived(&mut self, arrivals: &[Arrival]) {
        let mut woken: Vec<u64> = Vec::new();
        for arrival in arrivals {
            let Some(ids) = self.by_type.get(&arrival.flow_type) else {
                continue;
            };
            woken.extend(ids.iter().copied().filter(|id| {
                self.parked
                    .get(id)
                    .is_some_and(|parked| parked.matches(arrival))
            }));
        }
        for id in woken {
            self.resume(id, Wake::Retry);
        }
    }

    fn fire(&mut self, now: Instant) {
        while let Some(&(at, id)) = self.timers.first() {
            if at > now {
                break;
            }
            self.timers.remove(&(at, id));
            let expired = self
                .parked
                .get(&id)
                .is_some_and(|parked| parked.deadline.is_some_and(|deadline| deadline <= now));
            self.resume(id, if expired { Wake::Expired } else { Wake::Retry });
        }
    }

    fn cancelled(&mut self) {
        let gone: Vec<u64> = self
            .parked
            .iter()
            .filter(|(_, parked)| {
                parked
                    .cancellation
                    .as_ref()
                    .is_some_and(Cancellation::is_cancelled)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.resume(id, Wake::Cancelled);
        }
    }
}
