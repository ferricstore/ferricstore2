use openraft::type_config::TypeConfigExt;
use openraft::type_config::async_runtime::watch::WatchSender;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};

use openraft::storage::RaftSnapshotBuilder;
use openraft::{Snapshot, SnapshotMeta, StoredMembership};
use wf_ports::consensus::log::{
    Applied, Capture, Captured, ConsensusLogError, EntryId, ReplicatedMachine, SnapshotSource,
    SnapshotStore,
};
use wf_storage::snapshot::files::group::GroupSnapshots;

use super::super::codec;
use super::super::snapshot::{self, Source, Stored};
use super::super::testing::Root;
use super::{Bridge, Taken};

/// A machine that leads or follows as told; a follower's capture writes
/// `captured`.
#[derive(Default)]
struct Machine {
    leads: AtomicBool,
}

struct Written(&'static [u8]);

impl Capture for Written {
    fn write_to(self: Box<Self>, out: &mut dyn Write) -> Result<(), ConsensusLogError> {
        out.write_all(self.0)
            .map_err(|error| ConsensusLogError::new(error.to_string()))
    }
}

impl ReplicatedMachine for Machine {
    fn apply(&self, _proposals: Vec<Arc<[u8]>>, done: Applied) {
        done(Ok(()));
    }

    fn promote(&self, done: Applied) {
        done(Ok(()));
    }

    fn demote(&self) {}

    fn snapshot(&self, done: Captured) -> bool {
        if self.leads.load(Ordering::SeqCst) {
            return false;
        }
        done(Ok(Box::new(Written(b"captured"))));
        true
    }

    fn install(&self, _snapshot: Box<dyn SnapshotSource>, done: Applied) {
        done(Ok(()));
    }
}

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// A store of a fresh directory.
fn store() -> TestResult<Arc<dyn SnapshotStore>> {
    Ok(Arc::new(GroupSnapshots::open(Root::new().path())?))
}

/// A snapshot whose last index is `index`, named but not written.
fn at(store: &Arc<dyn SnapshotStore>, index: u64) -> Taken {
    Snapshot {
        meta: SnapshotMeta {
            last_log_id: Some(codec::log_id(EntryId {
                term: 1,
                node: 1,
                index,
            })),
            last_membership: StoredMembership::default(),
        },
        snapshot: Stored::new(Arc::clone(store), index, 0),
    }
}

fn bridge(leads: bool) -> TestResult<(Bridge, Arc<Machine>)> {
    let machine = Arc::new(Machine {
        leads: AtomicBool::new(leads),
    });
    let bridge = Bridge::new(Arc::clone(&machine) as _, (1, 7), store()?);
    Ok((bridge, machine))
}

/// The last index of the snapshot `bridge` would build now, and whether
/// from its machine, if it would build one.
fn captured(bridge: &Bridge) -> Option<(Option<u64>, bool)> {
    let capture = bridge.capture()?;
    let index = capture.meta.last_log_id.map(|log_id| log_id.index);
    let machine = matches!(capture.source, Some(Source::Machine(_)));
    Some((index, machine))
}

/// The bridge holds `snapshot` as its newest.
fn hold(bridge: &Bridge, snapshot: Taken) {
    *bridge
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some((snapshot.meta, snapshot.snapshot));
}

/// A leader's machine holds rounds not yet committed, so it builds only
/// from a follower's snapshot offered to it, once, and only one newer
/// than its own: an older one would leave it sending a snapshot its log no
/// longer follows.
#[test]
fn a_leader_builds_only_from_a_newer_offered_snapshot() -> TestResult {
    let (bridge, _machine) = bridge(true)?;
    let store = bridge.store();
    assert!(bridge.capture().is_none());
    bridge.offer(at(&store, 5));
    assert_eq!(captured(&bridge), Some((Some(5), false)));
    assert!(bridge.capture().is_none());
    hold(&bridge, at(&store, 10));
    bridge.offer(at(&store, 10));
    assert!(bridge.capture().is_none());
    bridge.offer(at(&store, 11));
    assert_eq!(captured(&bridge), Some((Some(11), false)));
    Ok(())
}

/// A follower captures its own machine at its applied position, and an
/// offer left from when it led is dropped rather than kept for later.
#[test]
fn a_follower_captures_its_machine_and_drops_an_offer() -> TestResult {
    let (bridge, machine) = bridge(false)?;
    let store = bridge.store();
    bridge.offer(at(&store, 5));
    assert_eq!(captured(&bridge), Some((None, true)));
    machine.leads.store(true, Ordering::SeqCst);
    assert!(bridge.capture().is_none());
    Ok(())
}

/// A follower's snapshot streams into its store: the head (its metadata)
/// and what the machine wrote, adopted as the newest, and held as current.
#[test]
fn a_follower_s_snapshot_is_a_stored_file() -> TestResult {
    let (bridge, _machine) = bridge(false)?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let mut capture = bridge.capture().ok_or("no capture")?;
    let Snapshot { meta, snapshot } = runtime.block_on(capture.build_snapshot())?;
    assert_eq!(bridge.store().newest()?, Some(snapshot.id()));
    let mut source = snapshot.open()?;
    assert_eq!(snapshot::read_head(&mut source)?, meta);
    let mut rest = Vec::new();
    source.read_to_end(&mut rest)?;
    assert_eq!(rest, b"captured");
    assert!(bridge.current.lock().is_ok_and(|current| current.is_some()));
    Ok(())
}

/// A machine whose promotion ends when the test lets it, and which counts
/// its demotions.
#[derive(Default)]
struct Slow {
    promoting: std::sync::Mutex<Option<Applied>>,
    demoted: std::sync::atomic::AtomicUsize,
}

impl ReplicatedMachine for Slow {
    fn apply(&self, _proposals: Vec<Arc<[u8]>>, done: Applied) {
        done(Ok(()));
    }

    fn promote(&self, done: Applied) {
        *self
            .promoting
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(done);
    }

    fn demote(&self) {
        self.demoted.fetch_add(1, Ordering::SeqCst);
    }

    fn snapshot(&self, _done: Captured) -> bool {
        false
    }

    fn install(&self, _snapshot: Box<dyn SnapshotSource>, done: Applied) {
        done(Ok(()));
    }
}

/// A member promotes its machine when it applies its own term's first
/// entry; a promotion that ends after another member took over is undone,
/// so the machine does not serve as a leader it no longer is.
#[test]
fn a_promotion_that_ends_after_leadership_moved_is_undone() -> TestResult {
    let machine = Arc::new(Slow::default());
    let bridge = Bridge::new(Arc::clone(&machine) as _, (1, 7), store()?);
    let mut metrics = openraft::RaftMetrics::new_initial(1);
    metrics.current_leader = Some(1);
    metrics.current_term = 3;
    let (publish, watched) =
        <super::super::Config as TypeConfigExt>::watch_channel(metrics.clone());
    let _set = bridge.metrics.set(watched);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let entry = openraft::Entry {
        log_id: codec::log_id(EntryId {
            term: 3,
            node: 1,
            index: 5,
        }),
        payload: openraft::EntryPayload::Blank,
    };
    let taking = runtime.spawn(async move {
        let mut pending = Vec::new();
        bridge.take(entry, &mut pending).await
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let done = loop {
        let held = machine
            .promoting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(done) = held {
            break done;
        }
        if std::time::Instant::now() > until {
            return Err("the machine was not promoted".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    metrics.current_leader = Some(2);
    metrics.current_term = 4;
    publish.send(metrics)?;
    done(Ok(()));
    runtime.block_on(taking)??;
    assert_eq!(machine.demoted.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Members started at once can share an epoch (the wall clock when each
/// started): a member skips only proposals it made itself in this process,
/// so another member's round with the same epoch is applied.
#[test]
fn another_member_s_round_with_the_same_epoch_is_applied() -> TestResult {
    let machine = Arc::new(Machine::default());
    let bridge = Bridge::new(Arc::clone(&machine) as _, (2, 7), store()?);
    let proposed_by = |node: u64| openraft::Entry {
        log_id: codec::log_id(EntryId {
            term: 2,
            node,
            index: 4,
        }),
        payload: openraft::EntryPayload::Normal(super::super::Proposal(super::wrap(7, b"round"))),
    };
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let mut pending = Vec::new();
    runtime.block_on(bridge.take(proposed_by(3), &mut pending))?;
    assert_eq!(pending, vec![Arc::<[u8]>::from(b"round".as_slice())]);
    // Its own round was applied when its shards decided it.
    let mut own = Vec::new();
    runtime.block_on(bridge.take(proposed_by(2), &mut own))?;
    assert_eq!(own, Vec::<Arc<[u8]>>::new());
    Ok(())
}
