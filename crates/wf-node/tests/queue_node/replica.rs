//! A replicated group of three nodes over loopback TCP (ADR-018 phase 2):
//! the leader's rounds reach the followers, a follower refuses client
//! requests as not led here, the group survives its leader's loss with every
//! acknowledged change, and a reopened member rejoins and catches up.

use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::time::{Duration, Instant};

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{CreateFlows, QueueService, ServiceError};
use wf_node::composition::{QueueNode, QueueNodeConfig, Replica};

use super::{TestResult, call, claim_request, completion, directory, flow as new_flow};

const MEMBERS: [u64; 3] = [1, 2, 3];

fn config() -> QueueNodeConfig {
    QueueNodeConfig {
        shards: 4,
        snapshot_segments: 0,
        schedule_poll: None,
        ..QueueNodeConfig::default()
    }
}

fn open(path: &Path, id: u64, peers: &BTreeMap<u64, SocketAddr>) -> TestResult<QueueNode> {
    let address = peers.get(&id).ok_or("unknown member")?;
    let replica = Replica {
        id,
        peers: peers.clone(),
        listener: TcpListener::bind(address)?,
    };
    Ok(QueueNode::open_replicated(
        &path.join(format!("n{id}")),
        config(),
        replica,
    )?)
}

fn create(node: &QueueNode, ids: &[String]) -> TestResult<Result<(), ServiceError>> {
    let request = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: None,
        priority: 0,
        flows: ids.iter().map(|id| new_flow(id)).collect(),
    };
    let outcomes = call(|reply| node.create(request, reply))?;
    Ok(outcomes.map(drop))
}

/// The member whose shards serve, once one does: a probe create succeeds
/// there and is refused as not led everywhere else.
fn serving(nodes: &BTreeMap<u64, QueueNode>, probe: &str) -> TestResult<u64> {
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        for (id, node) in nodes {
            if node.leader() == Some(*id) && create(node, &[probe.to_owned()])?.is_ok() {
                return Ok(*id);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("no member serves".into())
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    let options = Term::Map(vec![(b"id".to_vec(), Term::text(id))]);
    let command = flow::parse("FLOW.GET", &options)?;
    Ok(call(|reply| node.execute(command, reply))??)
}

fn state_of(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(get(node, id)?.get(b"state").cloned().unwrap_or(Term::Nil))
}

fn ids(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|index| format!("{prefix}{index}")).collect()
}

/// Every Flow `f*` is queued except the `completed` ones.
fn assert_replicated(node: &QueueNode, completed: &[String]) -> TestResult {
    for id in ids("f", 20) {
        let expected = if completed.contains(&id) {
            "completed"
        } else {
            "queued"
        };
        assert_eq!(state_of(node, &id)?, Term::text(expected), "{id}");
    }
    Ok(())
}

#[test]
fn a_group_keeps_its_flows_through_leader_loss_and_a_member_s_return() -> TestResult {
    let path = directory("replica")?;
    let mut peers = BTreeMap::new();
    for id in MEMBERS {
        // Bound once to pick a free port; each member binds it again.
        peers.insert(id, TcpListener::bind("127.0.0.1:0")?.local_addr()?);
    }
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let first = serving(&nodes, "probe-1")?;
    let leader = nodes.get(&first).ok_or("leader missing")?;
    create(leader, &ids("f", 20))??;
    let mut claim = claim_request(5);
    claim.flow_type = b"jobs".to_vec();
    let claimed = call(|reply| leader.claim(claim, reply))??;
    assert_eq!(claimed.len(), 5);
    let completed: Vec<String> = claimed
        .iter()
        .map(|job| String::from_utf8_lossy(&job.id).into_owned())
        .collect();
    let ordered: Vec<_> = claimed.iter().map(completion).collect();
    call(|reply| leader.complete(ordered, reply))??;
    let follower = nodes
        .iter()
        .find(|(id, _)| **id != first)
        .map(|(_, node)| node)
        .ok_or("no follower")?;
    assert_eq!(create(follower, &ids("x", 1))?, Err(ServiceError::Reroute));
    // The leader stops; the other two elect a new one with every change.
    drop(nodes.remove(&first));
    let second = serving(&nodes, "probe-2")?;
    assert_replicated(nodes.get(&second).ok_or("leader missing")?, &completed)?;
    // The first leader returns and catches up; without the second leader,
    // the remaining majority needs it.
    nodes.insert(first, open(&path, first, &peers)?);
    std::thread::sleep(Duration::from_secs(1));
    drop(nodes.remove(&second));
    let third = serving(&nodes, "probe-3")?;
    assert_replicated(nodes.get(&third).ok_or("leader missing")?, &completed)?;
    Ok(())
}
