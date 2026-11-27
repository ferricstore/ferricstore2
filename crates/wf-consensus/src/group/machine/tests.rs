use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};

use openraft::{Snapshot, SnapshotMeta, StoredMembership};
use wf_ports::consensus::log::{
    Applied, Captured, ConsensusLogError, EntryId, ReplicatedMachine, SnapshotStore,
};

use super::super::codec;
use super::{Bridge, Taken};

/// A machine that leads or follows as told; a follower's capture is
/// `captured`.
#[derive(Default)]
struct Machine {
    leads: AtomicBool,
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
        done(Ok(b"captured".to_vec()));
        true
    }

    fn install(&self, _snapshot: Vec<u8>, done: Applied) {
        done(Ok(()));
    }
}

/// Keeps nothing.
struct Discard;

impl SnapshotStore for Discard {
    fn save(&self, _id: u64, _parts: &[&[u8]]) -> Result<(), ConsensusLogError> {
        Ok(())
    }

    fn newest(&self) -> Result<Option<(u64, Vec<u8>)>, ConsensusLogError> {
        Ok(None)
    }
}

/// A snapshot whose last index is `index`; its bytes are the index.
fn at(index: u64) -> Taken {
    Snapshot {
        meta: SnapshotMeta {
            last_log_id: Some(codec::log_id(EntryId {
                term: 1,
                node: 1,
                index,
            })),
            last_membership: StoredMembership::default(),
        },
        snapshot: index.to_be_bytes().to_vec(),
    }
}

fn bridge(leads: bool) -> (Bridge, Arc<Machine>) {
    let machine = Arc::new(Machine {
        leads: AtomicBool::new(leads),
    });
    let bridge = Bridge::new(Arc::clone(&machine) as _, (1, 7), Arc::new(Discard));
    (bridge, machine)
}

/// The last index and bytes of the snapshot `bridge` would build now, if
/// it would build one.
fn captured(bridge: &Bridge) -> Option<(Option<u64>, Vec<u8>)> {
    let mut capture = bridge.capture()?;
    let index = capture.meta.last_log_id.map(|log_id| log_id.index);
    let bytes = capture.bytes.take()?.try_recv().ok()?.ok()?;
    Some((index, bytes))
}

/// The bridge holds `snapshot` as its newest.
fn hold(bridge: &Bridge, snapshot: Taken) {
    *bridge
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner) =
        Some((snapshot.meta, Arc::new(snapshot.snapshot)));
}

/// A leader's machine holds rounds not yet committed, so it builds only
/// from a follower's snapshot offered to it, once, and only one newer
/// than its own: an older one would leave it sending a snapshot its log no
/// longer follows.
#[test]
fn a_leader_builds_only_from_a_newer_offered_snapshot() {
    let (bridge, _machine) = bridge(true);
    assert!(bridge.capture().is_none());
    bridge.offer(at(5));
    assert_eq!(
        captured(&bridge),
        Some((Some(5), 5_u64.to_be_bytes().to_vec()))
    );
    assert!(bridge.capture().is_none());
    hold(&bridge, at(10));
    bridge.offer(at(10));
    assert!(bridge.capture().is_none());
    bridge.offer(at(11));
    assert_eq!(
        captured(&bridge),
        Some((Some(11), 11_u64.to_be_bytes().to_vec()))
    );
}

/// A follower captures its own machine at its applied position, and an
/// offer left from when it led is dropped rather than kept for later.
#[test]
fn a_follower_captures_its_machine_and_drops_an_offer() {
    let (bridge, machine) = bridge(false);
    bridge.offer(at(5));
    assert_eq!(captured(&bridge), Some((None, b"captured".to_vec())));
    machine.leads.store(true, Ordering::SeqCst);
    assert!(bridge.capture().is_none());
}
