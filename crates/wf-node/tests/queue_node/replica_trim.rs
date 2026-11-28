//! A leader that keeps leading trims its log (ADR-018 phase 3): it takes a
//! follower's snapshot as its own and drops its log behind it, so its log
//! stays bounded on disk and in memory; a member that log no longer
//! reaches installs the leader's snapshot, and the leader restarts from it.

use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::time::{Duration, Instant};

use wf_node::composition::{QueueNode, QueueNodeConfig, Replica};
use wf_storage::log::GroupLogLimits;

use super::replica::{MEMBERS, assert_states, batch, config, peers, serving, snapshotted};
use super::{TestResult, directory};

/// Opens member `id`: a follower snapshots every 20 entries, and the log
/// starts a segment every 4 KiB, so a trimmed log drops whole segments.
fn open(path: &Path, id: u64, peers: &BTreeMap<u64, SocketAddr>) -> TestResult<QueueNode> {
    open_framed(path, id, peers, wf_consensus::group::MAX_FRAME_BYTES)
}

/// As `open`, with frames between members of at most `max_frame_bytes`.
pub(super) fn open_framed(
    path: &Path,
    id: u64,
    peers: &BTreeMap<u64, SocketAddr>,
    max_frame_bytes: u32,
) -> TestResult<QueueNode> {
    let address = peers.get(&id).ok_or("unknown member")?;
    let replica = Replica {
        id,
        peers: peers
            .iter()
            .map(|(id, address)| (*id, address.to_string()))
            .collect(),
        voter: true,
        listener: TcpListener::bind(address)?,
        snapshot_every: 20,
        keep_logs: 0,
        max_frame_bytes,
    };
    let config = QueueNodeConfig {
        log: GroupLogLimits {
            max_segment_bytes: 4_096,
            ..config().log
        },
        ..config()
    };
    Ok(QueueNode::open_replicated(
        &path.join(format!("n{id}")),
        config,
        replica,
    )?)
}

/// Member `id`'s log segments, by number.
pub(super) fn segments(path: &Path, id: u64) -> TestResult<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path.join(format!("n{id}")).join("group"))? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// Whether member `id` keeps a snapshot and no longer keeps log segment
/// `first`.
fn trimmed(path: &Path, id: u64, first: &str) -> TestResult<bool> {
    let member = path.join(format!("n{id}"));
    let kept = std::fs::read_dir(member.join("group-snapshots"))?
        .next()
        .is_some();
    let dropped = segments(path, id)?
        .first()
        .is_none_or(|segment| segment.as_str() > first);
    Ok(kept && dropped)
}

/// Creates five Flows at a time through `node`, member `id`, until it
/// keeps a snapshot and dropped log segment `first`: its snapshots follow
/// its followers', which follow the entries they apply, so how many
/// entries that takes depends on how the rounds were cut.
pub(super) fn trim(path: &Path, (id, node): (u64, &QueueNode), first: &str) -> TestResult {
    let until = Instant::now() + Duration::from_secs(30);
    let mut round = 0;
    while Instant::now() < until {
        if trimmed(path, id, first)? {
            return Ok(());
        }
        batch(node, &format!("more{round}-"), 5, 0)?;
        round += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "member {id} keeps no snapshot or still keeps {first}: {:?}",
        segments(path, id)?
    )
    .into())
}

#[test]
fn a_leader_that_keeps_leading_trims_its_log() -> TestResult {
    let path = directory("replica-trim")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let a = serving(&nodes, "probe-a")?;
    let first = segments(&path, a)?.first().cloned().ok_or("no segment")?;
    let mut followers = MEMBERS.iter().copied().filter(|id| *id != a);
    let (b, c) = (
        followers.next().ok_or("no b")?,
        followers.next().ok_or("no c")?,
    );
    // C misses the first batch; B snapshots it as it follows.
    drop(nodes.remove(&c));
    let one = batch(nodes.get(&a).ok_or("no a")?, "t1-", 60, 10)?;
    // A leads throughout, yet keeps a snapshot and drops its log behind it.
    trim(&path, (a, nodes.get(&a).ok_or("no a")?), &first)?;
    // A's log no longer reaches C, so C installs A's snapshot.
    nodes.insert(c, open(&path, c, &peers)?);
    snapshotted(&path, c)?;
    // Without B, A and C take a second batch.
    drop(nodes.remove(&b));
    let led = serving(&nodes, "probe-b")?;
    let two = batch(nodes.get(&led).ok_or("no leader")?, "t2-", 10, 2)?;
    // Without A, only C has the second batch, so C leads with all of both.
    drop(nodes.remove(&a));
    nodes.insert(b, open(&path, b, &peers)?);
    let led = serving(&nodes, "probe-c")?;
    assert_eq!(led, c);
    let completed: Vec<String> = one.into_iter().chain(two).collect();
    assert_eq!(completed.len(), 12);
    let leader = nodes.get(&c).ok_or("no c")?;
    assert_states(leader, "t1-", 60, &completed)?;
    assert_states(leader, "t2-", 10, &completed)?;
    // A restarts from the snapshot it took and its log; without C, A and B
    // serve everything.
    nodes.insert(a, open(&path, a, &peers)?);
    std::thread::sleep(Duration::from_secs(1));
    drop(nodes.remove(&c));
    let led = serving(&nodes, "probe-d")?;
    let leader = nodes.get(&led).ok_or("no leader")?;
    assert_states(leader, "t1-", 60, &completed)?;
    assert_states(leader, "t2-", 10, &completed)?;
    Ok(())
}
