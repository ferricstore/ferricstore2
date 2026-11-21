//! Backpressure (ADR-017): each shard counts the items waiting for it. New
//! work for a shard at its limit is refused as busy before any part of the
//! request runs; other work is admitted up to a higher limit. A slow log
//! holds the shards back, so the backlog builds up instead of memory.

use std::io;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{CreateFlows, QueueService, ServiceError};
use wf_node::composition::{Admission, QueueNode, QueueNodeConfig};
use wf_storage::log::{DirectoryStorage, GroupLogLimits, SegmentStorage, SegmentWriter};

use super::{TestResult, call, claim_request, directory, flow as new_flow};

/// Holds log syncs while closed.
struct Gate {
    state: Mutex<(bool, usize)>,
    changed: Condvar,
}

impl Gate {
    fn set(&self, held: bool) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0 = held;
        self.changed.notify_all();
    }

    fn started(&self) -> usize {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    /// Waits until more than `count` syncs have started.
    fn wait_started(&self, count: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while state.1 <= count {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    fn sync(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.1 = state.1.saturating_add(1);
        self.changed.notify_all();
        while state.0 {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Lets held syncs pass when dropped, so a failed assertion does not leave
/// the node's drop waiting on its log.
struct Release(Arc<Gate>);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct GatedStorage {
    inner: DirectoryStorage,
    gate: Arc<Gate>,
}

struct GatedWriter {
    inner: Box<dyn SegmentWriter>,
    gate: Arc<Gate>,
}

impl SegmentStorage for GatedStorage {
    fn segments(&self) -> io::Result<Vec<u64>> {
        self.inner.segments()
    }
    fn read(&self, segment: u64) -> io::Result<Vec<u8>> {
        self.inner.read(segment)
    }
    fn create(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        Ok(Box::new(GatedWriter {
            inner: self.inner.create(segment)?,
            gate: Arc::clone(&self.gate),
        }))
    }
    fn reopen(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        Ok(Box::new(GatedWriter {
            inner: self.inner.reopen(segment)?,
            gate: Arc::clone(&self.gate),
        }))
    }
    fn truncate(&self, segment: u64, length: u64) -> io::Result<()> {
        self.inner.truncate(segment, length)
    }
    fn remove(&self, segment: u64) -> io::Result<()> {
        self.inner.remove(segment)
    }
}

impl SegmentWriter for GatedWriter {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.append(bytes)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.gate.sync();
        self.inner.sync()
    }
}

fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

type Pending = Receiver<Result<Term, ServiceError>>;

/// Starts a command; its reply arrives on the returned channel.
fn send(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Pending> {
    let command = flow::parse(name, &options(entries))?;
    let (sender, receiver) = mpsc::channel();
    node.execute(
        command,
        Box::new(move |result| {
            let _delivered = sender.send(result);
        }),
    );
    Ok(receiver)
}

fn create(node: &QueueNode, id: &str, payload: usize) -> TestResult<Pending> {
    send(
        node,
        "FLOW.CREATE",
        &[
            ("id", Term::text(id)),
            ("type", Term::text("jobs")),
            ("partition_key", Term::text("p")),
            ("payload", Term::Bin(vec![b'x'; payload])),
        ],
    )
}

fn get(node: &QueueNode, id: &str) -> TestResult<Pending> {
    send(
        node,
        "FLOW.GET",
        &[("id", Term::text(id)), ("partition_key", Term::text("p"))],
    )
}

fn quiet(admission: Admission) -> QueueNodeConfig {
    QueueNodeConfig {
        shards: 1,
        admission,
        log: GroupLogLimits {
            max_queued_bytes: 1_000,
            ..QueueNodeConfig::default().log
        },
        snapshot_segments: 0,
        schedule_poll: None,
        ..QueueNodeConfig::default()
    }
}

#[test]
fn new_work_behind_a_slow_log_is_refused_busy_once_the_backlog_is_full() -> TestResult {
    let path = directory("backpressure")?;
    let gate = Arc::new(Gate {
        state: Mutex::new((false, 0)),
        changed: Condvar::new(),
    });
    let storage = GatedStorage {
        inner: DirectoryStorage::open(&path.join("log"))?,
        gate: Arc::clone(&gate),
    };
    let admission = Admission {
        new_work: 8,
        other: 1_000,
    };
    let node = QueueNode::open_with_log(&path, quiet(admission), Arc::new(storage))?;
    let _release = Release(Arc::clone(&gate));
    gate.set(true);
    let syncs = gate.started();
    // The first create's batch is held in the log's sync; the second goes
    // past the write queue's limit, so the shard's next append waits.
    let mut admitted = vec![("a".to_owned(), create(&node, "a", 10)?)];
    assert!(gate.wait_started(syncs), "the log holds the first batch");
    admitted.push(("b".to_owned(), create(&node, "b", 2_000)?));
    let mut refused = None;
    // Sends are paced, so a shard the log does not hold back keeps its
    // backlog empty; a held shard's fills within two rounds (at most 256
    // requests each) and the limit.
    for index in 0..600 {
        std::thread::sleep(Duration::from_millis(2));
        let id = format!("n{index}");
        let pending = create(&node, &id, 10)?;
        match pending.try_recv() {
            Ok(Err(ServiceError::Busy)) => {
                refused = Some(id);
                break;
            }
            Ok(other) => return Err(format!("{id} answered early: {other:?}").into()),
            Err(_) => admitted.push((id, pending)),
        }
    }
    let refused = refused.ok_or("no create was refused")?;
    let read = get(&node, "a")?;
    assert!(read.try_recv().is_err(), "a read is still admitted");
    gate.set(false);
    for (id, pending) in admitted {
        assert!(
            pending.recv_timeout(Duration::from_secs(20))?.is_ok(),
            "{id}"
        );
    }
    let record = read.recv_timeout(Duration::from_secs(20))??;
    assert_eq!(record.get(b"id"), Some(&Term::text("a")));
    let missing = get(&node, &refused)?.recv_timeout(Duration::from_secs(20))??;
    assert_eq!(missing, Term::Nil, "a refused create ran nowhere");
    Ok(())
}

#[test]
fn without_room_for_new_work_every_create_form_is_refused_and_other_work_runs() -> TestResult {
    let path = directory("backpressure-classes")?;
    let admission = Admission {
        new_work: 0,
        ..Admission::default()
    };
    let node = super::open_with(&path, quiet(admission))?;
    let busy = |pending: Pending| -> TestResult<bool> {
        Ok(matches!(
            pending.recv_timeout(Duration::from_secs(20))?,
            Err(ServiceError::Busy)
        ))
    };
    assert!(busy(create(&node, "c1", 1)?)?, "FLOW.CREATE");
    let items = Term::List(vec![Term::List(vec![Term::text("m1")])]);
    let many = send(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("type", Term::text("jobs")),
            ("partition_key", Term::text("p")),
            ("items", items),
        ],
    )?;
    assert!(busy(many)?, "FLOW.CREATE_MANY");
    let start = send(
        &node,
        "FLOW.START_AND_CLAIM",
        &[
            ("id", Term::text("s1")),
            ("type", Term::text("jobs")),
            ("initial_state", Term::text("step1")),
            ("worker", Term::text("w")),
            ("lease_ms", Term::Int(1_000)),
        ],
    )?;
    assert!(busy(start)?, "FLOW.START_AND_CLAIM");
    let queue_create = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: None,
        priority: 0,
        flows: vec![new_flow("q1")],
    };
    assert_eq!(
        call(|reply| node.create(queue_create, reply))?,
        Err(ServiceError::Busy),
        "queue create"
    );
    assert_eq!(
        get(&node, "c1")?.recv_timeout(Duration::from_secs(20))??,
        Term::Nil
    );
    assert_eq!(
        call(|reply| node.claim(claim_request(1), reply))??,
        Vec::new()
    );
    Ok(())
}
