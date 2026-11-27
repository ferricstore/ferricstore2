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
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let path = std::env::temp_dir().join(format!("wf-bridge-tests-{}-{nanos}", std::process::id()));
    Ok(Arc::new(GroupSnapshots::open(&path)?))
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
