//! A leader that no quorum acknowledged within its lease holds its rounds
//! (ADR-018): `OpenRaft` refuses a leader's writes once its lease lapsed,
//! and the leader's shards had already applied a refused round, so the
//! node stopped. Without both followers for longer than the lease, a
//! leader's round now waits for one of them and commits.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::Duration;

use wf_application::commands::queue::{CreateFlows, QueueService};

use super::replica::{MEMBERS, open, peers, serving};
use super::{TestResult, directory, flow};

#[test]
fn a_leader_without_a_quorum_holds_its_rounds_until_one_returns() -> TestResult {
    let path = directory("replica-lease")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let a = serving(&nodes, "probe-a")?;
    let leader = nodes.remove(&a).ok_or("no a")?;
    let followers: Vec<u64> = nodes.keys().copied().collect();
    nodes.clear();
    // Longer than the leader's lease (one second): no quorum acknowledged
    // it since.
    std::thread::sleep(Duration::from_secs(2));
    let request = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: None,
        priority: 0,
        flows: vec![flow("held")],
    };
    let (sender, receiver) = mpsc::channel();
    leader.create(
        request,
        Box::new(move |outcome| {
            // The test may have stopped waiting.
            let _sent = sender.send(outcome);
        }),
    );
    let back = followers.first().copied().ok_or("no follower")?;
    nodes.insert(back, open(&path, back, &peers)?);
    let created = receiver.recv_timeout(Duration::from_secs(20))?;
    assert!(created.is_ok(), "{created:?}");
    assert!(!leader.poisoned());
    Ok(())
}
