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
        // Replaced payloads go to disk, so an image carries them from there
        // (ADR-016 phase D).
        kept_memory_bytes: 0,
        ..QueueNodeConfig::default()
    }
}

fn open(path: &Path, id: u64, peers: &BTreeMap<u64, SocketAddr>) -> TestResult<QueueNode> {
    open_with(path, id, peers, 0)
}

/// Opens member `id`; a follower snapshots every `snapshot_every` entries
/// and keeps no log behind its snapshot.
fn open_with(
    path: &Path,
    id: u64,
    peers: &BTreeMap<u64, SocketAddr>,
    snapshot_every: u64,
) -> TestResult<QueueNode> {
    let address = peers.get(&id).ok_or("unknown member")?;
    let replica = Replica {
        id,
        peers: peers
            .iter()
            .map(|(id, address)| (*id, address.to_string()))
            .collect(),
        listener: TcpListener::bind(address)?,
        snapshot_every,
        keep_logs: 0,
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

fn run(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Term> {
    let options = entries
        .iter()
        .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
        .collect();
    let command = flow::parse(name, &Term::Map(options))?;
    Ok(call(|reply| node.execute(command, reply))??)
}

/// A payload long enough to go to disk once replaced.
fn long_payload(at: usize) -> String {
    format!("v{at}-").repeat(40)
}

/// Creates Flow `kept` and replaces its payload three times: its payload
/// references and payloads, oldest first, as `FLOW.VALUE.MGET` takes and
/// answers them.
fn kept_versions(node: &QueueNode) -> TestResult<(Term, Term)> {
    let reference = |node: &QueueNode| -> TestResult<Term> {
        Ok(get(node, "kept")?
            .get(b"payload_ref")
            .cloned()
            .unwrap_or(Term::Nil))
    };
    let create = [
        ("id", Term::text("kept")),
        ("type", Term::text("kept")),
        ("state", Term::text("a")),
        ("payload", Term::text(&long_payload(0))),
    ];
    run(node, "FLOW.CREATE", &create)?;
    let (mut refs, mut payloads) = (vec![reference(node)?], vec![Term::text(&long_payload(0))]);
    for (at, (from, to)) in [("a", "b"), ("b", "a"), ("a", "b")].into_iter().enumerate() {
        let payload = long_payload(at.saturating_add(1));
        let transition = [
            ("id", Term::text("kept")),
            ("from_state", Term::text(from)),
            ("to_state", Term::text(to)),
            ("fencing_token", Term::Int(0)),
            ("payload", Term::text(&payload)),
        ];
        run(node, "FLOW.TRANSITION", &transition)?;
        refs.push(reference(node)?);
        payloads.push(Term::text(&payload));
    }
    Ok((Term::List(refs), Term::List(payloads)))
}

/// Moves three new Flows `shared-*` with one payload, which the leader
/// logs once for the three: the payload.
fn shared_batch(node: &QueueNode) -> TestResult<Term> {
    let ids: Vec<Term> = (0..3)
        .map(|index| Term::text(&format!("shared-{index}")))
        .collect();
    let create = [
        ("partition_key", Term::text("shared")),
        ("type", Term::text("shared")),
        ("state", Term::text("a")),
        ("items", Term::List(ids.clone())),
    ];
    run(node, "FLOW.CREATE_MANY", &create)?;
    let items = ids
        .into_iter()
        .map(|id| {
            Term::Map(vec![
                (b"id".to_vec(), id),
                (b"fencing_token".to_vec(), Term::Int(0)),
            ])
        })
        .collect();
    let payload = Term::text(&"s".repeat(512));
    let transition = [
        ("partition_key", Term::text("shared")),
        ("from_state", Term::text("a")),
        ("to_state", Term::text("b")),
        ("items", Term::List(items)),
        ("payload", payload.clone()),
    ];
    run(node, "FLOW.TRANSITION_MANY", &transition)?;
    Ok(payload)
}

fn assert_shared_batch(node: &QueueNode, payload: &Term) -> TestResult {
    for index in 0..3 {
        let get = [
            ("id", Term::text(&format!("shared-{index}"))),
            ("partition_key", Term::text("shared")),
            ("full", Term::Bool(true)),
        ];
        assert_eq!(run(node, "FLOW.GET", &get)?.get(b"payload"), Some(payload));
    }
    Ok(())
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

fn peers() -> TestResult<BTreeMap<u64, SocketAddr>> {
    let mut peers = BTreeMap::new();
    for id in MEMBERS {
        // Bound once to pick a free port; each member binds it again.
        peers.insert(id, TcpListener::bind("127.0.0.1:0")?.local_addr()?);
    }
    Ok(peers)
}

/// Waits until member `id` has taken a snapshot.
fn snapshotted(path: &Path, id: u64) -> TestResult {
    let directory = path.join(format!("n{id}")).join("group-snapshots");
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        if std::fs::read_dir(&directory)?.next().is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("member {id} took no snapshot").into())
}

/// Creates `prefix0..` through `node` and completes the Flows it claims.
fn batch(node: &QueueNode, prefix: &str, count: usize, done: usize) -> TestResult<Vec<String>> {
    for chunk in ids(prefix, count).chunks(5) {
        create(node, chunk)??;
    }
    if done == 0 {
        return Ok(Vec::new());
    }
    let claimed = call(|reply| node.claim(claim_request(done), reply))??;
    let completions: Vec<_> = claimed.iter().map(completion).collect();
    call(|reply| node.complete(completions, reply))??;
    Ok(claimed
        .iter()
        .map(|job| String::from_utf8_lossy(&job.id).into_owned())
        .collect())
}

fn assert_states(node: &QueueNode, prefix: &str, count: usize, completed: &[String]) -> TestResult {
    for id in ids(prefix, count) {
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
fn a_member_the_leader_s_log_no_longer_reaches_installs_a_snapshot() -> TestResult {
    let path = directory("replica-snapshot")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open_with(&path, id, &peers, 20)?);
    }
    let a = serving(&nodes, "probe-a")?;
    let mut followers = MEMBERS.iter().copied().filter(|id| *id != a);
    let (b, c) = (
        followers.next().ok_or("no b")?,
        followers.next().ok_or("no c")?,
    );
    // C misses the first batch; B snapshots it and drops its log.
    drop(nodes.remove(&c));
    // A Flow's kept value versions reach C in B's image too.
    let (refs, payloads) = kept_versions(nodes.get(&a).ok_or("no a")?)?;
    let shared = shared_batch(nodes.get(&a).ok_or("no a")?)?;
    let first = batch(nodes.get(&a).ok_or("no a")?, "w1-", 40, 10)?;
    snapshotted(&path, b)?;
    // Without A, only B is up to date, so it leads, and C needs B's snapshot.
    drop(nodes.remove(&a));
    nodes.insert(c, open_with(&path, c, &peers, 20)?);
    let led = serving(&nodes, "probe-b")?;
    assert_eq!(led, b);
    // B applied A's round, whose three after-images shared one record.
    assert_shared_batch(nodes.get(&b).ok_or("no b")?, &shared)?;
    let second = batch(nodes.get(&b).ok_or("no b")?, "w2-", 10, 2)?;
    // Without B, A lacks the second batch, so C leads with all of it.
    drop(nodes.remove(&b));
    nodes.insert(a, open_with(&path, a, &peers, 20)?);
    let led = serving(&nodes, "probe-c")?;
    assert_eq!(led, c);
    let leader = nodes.get(&c).ok_or("no c")?;
    // The second batch's claims may take Flows of the first.
    let completed: Vec<String> = first.into_iter().chain(second).collect();
    assert_eq!(completed.len(), 12);
    assert_states(leader, "w1-", 40, &completed)?;
    assert_states(leader, "w2-", 10, &completed)?;
    let read = [("refs", refs)];
    assert_eq!(run(leader, "FLOW.VALUE.MGET", &read)?, payloads);
    // B restarts from its snapshot and log and takes a third batch; without
    // C, only B has it, so B leads with everything.
    drop(nodes.remove(&a));
    nodes.insert(b, open_with(&path, b, &peers, 20)?);
    let third = batch(nodes.get(&c).ok_or("no c")?, "w3-", 10, 0)?;
    assert_eq!(third, Vec::<String>::new());
    drop(nodes.remove(&c));
    nodes.insert(a, open_with(&path, a, &peers, 20)?);
    let led = serving(&nodes, "probe-d")?;
    assert_eq!(led, b);
    let leader = nodes.get(&b).ok_or("no b")?;
    assert_states(leader, "w1-", 40, &completed)?;
    assert_states(leader, "w2-", 10, &completed)?;
    assert_states(leader, "w3-", 10, &completed)?;
    Ok(())
}

#[test]
fn the_lowest_member_that_lost_its_data_rejoins_instead_of_forming_a_group() -> TestResult {
    let path = directory("replica-wiped")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let first = serving(&nodes, "probe-1")?;
    create(nodes.get(&first).ok_or("no leader")?, &ids("k", 10))??;
    // Member 1 loses its data while the others keep the group.
    drop(nodes.remove(&1));
    std::fs::remove_dir_all(path.join("n1"))?;
    nodes.insert(1, open(&path, 1, &peers)?);
    let leader = serving(&nodes, "probe-2")?;
    std::thread::sleep(Duration::from_secs(1));
    // The next majority needs member 1: every Flow must still be there.
    let other = MEMBERS
        .iter()
        .copied()
        .find(|id| *id != 1 && *id != leader)
        .ok_or("no third member")?;
    drop(nodes.remove(&other));
    let next = serving(&nodes, "probe-3")?;
    let node = nodes.get(&next).ok_or("no leader")?;
    for id in ids("k", 10) {
        assert_eq!(state_of(node, &id)?, Term::text("queued"), "{id}");
    }
    Ok(())
}

/// A member that lost its data votes only once it caught up: Raft's safety
/// rests on a voter keeping its log, and here its vote would elect a member
/// that lacks entries only the leader and it held, which would be lost.
#[test]
fn a_member_that_lost_its_data_does_not_vote_until_it_caught_up() -> TestResult {
    let path = directory("replica-amnesia")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let leader = serving(&nodes, "probe-1")?;
    let mut followers = MEMBERS.iter().copied().filter(|id| *id != leader);
    let lagging = followers.next().ok_or("no follower")?;
    let holder = followers.next().ok_or("no second follower")?;
    // Without `lagging`, the writes commit on the leader and `holder` only.
    drop(nodes.remove(&lagging));
    create(nodes.get(&leader).ok_or("no leader")?, &ids("k", 10))??;
    drop(nodes.remove(&leader));
    drop(nodes.remove(&holder));
    std::fs::remove_dir_all(path.join(format!("n{holder}")))?;
    nodes.insert(lagging, open(&path, lagging, &peers)?);
    nodes.insert(holder, open(&path, holder, &peers)?);
    // Only the stopped leader holds the entries now: no one may lead.
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        for (id, node) in &nodes {
            assert_ne!(
                node.leader(),
                Some(*id),
                "member {id} led without the entries"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    nodes.insert(leader, open(&path, leader, &peers)?);
    let next = serving(&nodes, "probe-2")?;
    let node = nodes.get(&next).ok_or("no leader")?;
    for id in ids("k", 10) {
        assert_eq!(state_of(node, &id)?, Term::text("queued"), "{id}");
    }
    Ok(())
}

/// A follower that lost its data while the same member leads catches up
/// from that leader: the leader starts over the progress it had recorded
/// for the follower, and the group keeps committing through it.
#[test]
fn a_follower_that_lost_its_data_catches_up_under_the_same_leader() -> TestResult {
    let path = directory("replica-wiped-follower")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let leader = serving(&nodes, "probe-1")?;
    create(nodes.get(&leader).ok_or("no leader")?, &ids("k", 10))??;
    let mut followers = MEMBERS.iter().copied().filter(|id| *id != leader);
    let wiped = followers.next().ok_or("no follower")?;
    let other = followers.next().ok_or("no second follower")?;
    drop(nodes.remove(&wiped));
    std::fs::remove_dir_all(path.join(format!("n{wiped}")))?;
    nodes.insert(wiped, open(&path, wiped, &peers)?);
    // Without the other follower, a write commits only through the wiped
    // one.
    drop(nodes.remove(&other));
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let node = nodes.get(&leader).ok_or("no leader")?;
        if create(node, &ids("m", 1))?.is_ok() {
            break;
        }
        if Instant::now() > until {
            return Err("no write committed through the wiped follower".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // The wiped follower now holds every Flow: with the leader gone, it and
    // the other follower lead with all of them.
    drop(nodes.remove(&leader));
    nodes.insert(other, open(&path, other, &peers)?);
    let next = serving(&nodes, "probe-2")?;
    let node = nodes.get(&next).ok_or("no leader")?;
    for id in ids("k", 10).into_iter().chain(ids("m", 1)) {
        assert_eq!(state_of(node, &id)?, Term::text("queued"), "{id}");
    }
    Ok(())
}
