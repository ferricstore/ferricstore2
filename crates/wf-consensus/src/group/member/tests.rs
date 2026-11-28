use openraft::type_config::TypeConfigExt;
use openraft::type_config::async_runtime::watch::WatchSender;
use std::collections::BTreeMap;
use std::sync::Arc;

use wf_ports::consensus::log::{ConsensusLog, SnapshotStore};
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_storage::snapshot::files::group::GroupSnapshots;

use super::super::MAX_FRAME_BYTES;
use super::super::testing::{Machine, Root, TestResult, runtime};
use super::{Member, MemberConfig, due, watch};

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

/// A machine that counts its demotions.
#[derive(Default)]
struct Counted {
    demoted: std::sync::atomic::AtomicUsize,
}

impl wf_ports::consensus::log::ReplicatedMachine for Counted {
    fn apply(&self, _proposals: Vec<Arc<[u8]>>, done: wf_ports::consensus::log::Applied) {
        done(Ok(()));
    }

    fn promote(&self, done: wf_ports::consensus::log::Applied) {
        done(Ok(()));
    }

    fn demote(&self) {
        self.demoted
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn snapshot(&self, _done: wf_ports::consensus::log::Captured) -> bool {
        false
    }

    fn install(
        &self,
        _snapshot: Box<dyn wf_ports::consensus::log::SnapshotSource>,
        done: wf_ports::consensus::log::Applied,
    ) {
        done(Ok(()));
    }
}

/// A member that does not lead keeps its machine demoted, whatever it saw
/// before: a leadership too brief for the watcher to see still ends
/// demoted, since the machine may have been promoted meanwhile.
#[test]
fn a_member_that_does_not_lead_keeps_its_machine_demoted() -> TestResult {
    let mut metrics = openraft::RaftMetrics::<super::super::Config>::new_initial(1);
    metrics.current_leader = Some(2);
    let (publish, watched) =
        <super::super::Config as TypeConfigExt>::watch_channel(metrics.clone());
    let machine = Arc::new(Counted::default());
    let runtime = runtime()?;
    let watching = runtime.spawn(watch(watched, 1, Arc::clone(&machine) as _));
    let demoted = |at_least: usize| -> bool {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < until {
            if machine.demoted.load(std::sync::atomic::Ordering::SeqCst) >= at_least {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    };
    assert!(demoted(1), "a member another leads is demoted");
    metrics.current_leader = Some(3);
    metrics.current_term = 2;
    publish.send(metrics)?;
    assert!(demoted(2), "and again when it learns of a new leader");
    watching.abort();
    Ok(())
}
