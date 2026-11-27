use std::collections::BTreeMap;
use std::sync::Arc;

use wf_ports::consensus::log::{ConsensusLog, SnapshotStore};
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_storage::snapshot::files::group::GroupSnapshots;

use super::super::MAX_FRAME_BYTES;
use super::super::testing::{Machine, Root, TestResult, runtime};
use super::{Member, MemberConfig, due};

/// A member takes its next snapshot once it applied `every` entries past
/// its newest, not before.
#[test]
fn a_snapshot_is_due_every_so_many_applied_entries() {
    assert!(!due(19, 0, 20));
    assert!(due(20, 0, 20));
    assert!(due(25, 0, 20));
    assert!(!due(39, 20, 20));
    assert!(due(40, 20, 20));
    // A snapshot past the applied entries (installed from the leader) is
    // not followed by another at once.
    assert!(!due(10, 30, 20));
}

/// A stopped member's log and snapshot store take no more changes, so its
/// work still running (a vote waiting on its disk) changes nothing more
/// there for whatever opens them next.
#[test]
fn a_stopped_member_s_log_and_snapshot_store_take_no_more_changes() -> TestResult {
    let root = Root::new();
    // The store makes the root; the log makes only its own directory.
    let store = Arc::new(GroupSnapshots::open(&root.path().join("snapshots"))?);
    let log = Arc::new(EntryLog::open(
        &root.path().join("log"),
        GroupLogLimits::default(),
    )?);
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let config = MemberConfig {
        id: 1,
        peers: BTreeMap::from([(1, listener.local_addr()?.to_string())]),
        listener,
        snapshot_every: 0,
        keep_logs: 10,
        max_frame_bytes: MAX_FRAME_BYTES,
    };
    let stores: (Arc<dyn ConsensusLog>, Arc<dyn SnapshotStore>) =
        (Arc::clone(&log) as _, Arc::clone(&store) as _);
    let runtime = runtime()?;
    let (member, _proposer) = Member::start(runtime.handle(), config, stores, Arc::new(Machine))?;
    member.shutdown();
    assert!(log.save_vote(b"vote").is_err());
    assert!(store.create(1).is_err());
    Ok(())
}
