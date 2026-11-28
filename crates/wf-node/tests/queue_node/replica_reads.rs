//! Reads on any member (ADR-018): a follower answers a read-only command
//! once it applied everything the leader had committed when the read
//! arrived, so it sees every write the leader acknowledged before; writes
//! still go to the leader.

use std::collections::BTreeMap;

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::replica::{MEMBERS, open, peers, serving};
use super::{TestResult, call, directory};

fn object(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: Vec<(&str, Term)>,
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &object(entries))?;
    call(|reply| node.execute(command, reply))
}

pub(super) fn create(node: &QueueNode, id: &str) -> TestResult<Result<Term, ServiceError>> {
    run(
        node,
        "FLOW.CREATE",
        vec![
            ("id", Term::text(id)),
            ("type", Term::text("reads")),
            ("partition_key", Term::text("p")),
            ("state", Term::text("ready")),
        ],
    )
}

pub(super) fn state(node: &QueueNode, id: &str) -> TestResult<Result<Term, ServiceError>> {
    let found = run(
        node,
        "FLOW.GET",
        vec![("id", Term::text(id)), ("partition_key", Term::text("p"))],
    )?;
    Ok(found.map(|flow| flow.get(b"state").cloned().unwrap_or(Term::Nil)))
}

fn queried(node: &QueueNode) -> TestResult<Result<usize, ServiceError>> {
    // The SDK failover test's query.
    let query = "FROM runs WHERE partition_key = @partition AND type = @type \
                 AND state = @state ORDER BY updated_at_ms ASC LIMIT 100 RETURN RECORDS";
    let result = run(
        node,
        "FLOW.QUERY",
        vec![
            ("version", Term::text("FQL1")),
            ("query", Term::text(query)),
            (
                "params",
                object(vec![
                    ("partition", Term::text("p")),
                    ("type", Term::text("reads")),
                    ("state", Term::text("ready")),
                ]),
            ),
        ],
    )?;
    Ok(result.map(|term| match term.get(b"records") {
        Some(Term::List(records)) => records.len(),
        _ => 0,
    }))
}

#[test]
fn a_follower_answers_reads_with_every_acknowledged_write() -> TestResult {
    let path = directory("replica-reads")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let led = serving(&nodes, "probe")?;
    let leader = nodes.get(&led).ok_or("no leader")?;
    let mut written = 0;
    for (id, follower) in nodes.iter().filter(|(id, _)| **id != led) {
        for round in 0..10 {
            let flow = format!("r{id}-{round}");
            create(leader, &flow)??;
            written += 1;
            // At once: the follower's read waits for what the leader had
            // committed, so it sees the write just acknowledged.
            assert_eq!(state(follower, &flow)?, Ok(Term::text("ready")));
            assert_eq!(queried(follower)?, Ok(written));
        }
        assert_eq!(
            create(follower, &format!("w{id}"))?,
            Err(ServiceError::Reroute)
        );
    }
    Ok(())
}

/// A member that cannot reach a quorum answers no read, rather than one
/// that may miss writes a new leader acknowledged since: with the leader
/// and another member gone, the last one refuses as not led here.
#[test]
fn a_member_without_a_quorum_answers_no_read() -> TestResult {
    let path = directory("replica-reads-alone")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let led = serving(&nodes, "probe")?;
    create(nodes.get(&led).ok_or("no leader")?, "kept")??;
    let alone = MEMBERS
        .into_iter()
        .find(|id| *id != led)
        .ok_or("no follower")?;
    let last = nodes.remove(&alone).ok_or("no follower")?;
    assert_eq!(state(&last, "kept")?, Ok(Term::text("ready")));
    nodes.clear();
    assert_eq!(state(&last, "kept")?, Err(ServiceError::Reroute));
    assert_eq!(queried(&last)?, Err(ServiceError::Reroute));
    Ok(())
}
