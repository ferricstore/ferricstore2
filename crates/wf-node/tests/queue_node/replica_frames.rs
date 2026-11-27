//! Peer messages larger than a frame (ADR-018): an append whose entries do
//! not fit one frame goes in parts, and a snapshot's bytes go in frames of
//! their own, so a member catches up whatever the size of what it missed.
//! Members here allow 64 KiB frames; each Flow carries 8 KiB, so what a
//! member misses spans several frames.

use std::collections::BTreeMap;
use std::path::Path;

use wf_application::commands::queue::{CreateFlows, NewFlow, QueueService};
use wf_node::composition::QueueNode;

use super::replica::{MEMBERS, assert_states, peers, serving, snapshotted};
use super::replica_trim::{open_framed, segments, trim};
use super::{TestResult, call, directory};

const FRAME: u32 = 64 * 1024;

/// Creates `prefix0..` through `node`, five at a time, each with an 8 KiB
/// payload.
fn heavy(node: &QueueNode, prefix: &str, count: usize) -> TestResult {
    for start in (0..count).step_by(5) {
        let flows = (start..count.min(start + 5))
            .map(|index| -> TestResult<NewFlow> {
                Ok(NewFlow {
                    id: format!("{prefix}{index}").into_bytes(),
                    partition: None,
                    payload: Some(vec![u8::try_from(index % 251)?; 8 * 1024]),
                })
            })
            .collect::<TestResult<Vec<_>>>()?;
        let request = CreateFlows {
            flow_type: b"jobs".to_vec(),
            state: b"queued".to_vec(),
            run_at_ms: None,
            priority: 0,
            flows,
        };
        call(|reply| node.create(request, reply))??;
    }
    Ok(())
}

fn open(
    path: &Path,
    id: u64,
    peers: &BTreeMap<u64, std::net::SocketAddr>,
) -> TestResult<QueueNode> {
    open_framed(path, id, peers, FRAME)
}

#[test]
fn a_follower_catches_up_on_entries_larger_than_a_frame() -> TestResult {
    let path = directory("replica-frames-log")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let a = serving(&nodes, "probe-a")?;
    let mut others = MEMBERS.iter().copied().filter(|id| *id != a);
    let (b, c) = (others.next().ok_or("no b")?, others.next().ok_or("no c")?);
    drop(nodes.remove(&c));
    // Fewer entries than a snapshot takes: C catches up from the log.
    heavy(nodes.get(&a).ok_or("no a")?, "h", 15)?;
    nodes.insert(c, open(&path, c, &peers)?);
    // Without B, A needs C to commit.
    drop(nodes.remove(&b));
    let led = serving(&nodes, "probe-b")?;
    assert_states(nodes.get(&led).ok_or("no leader")?, "h", 15, &[])?;
    Ok(())
}

#[test]
fn a_member_installs_a_snapshot_larger_than_a_frame() -> TestResult {
    let path = directory("replica-frames-snapshot")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open(&path, id, &peers)?);
    }
    let a = serving(&nodes, "probe-a")?;
    let first = segments(&path, a)?.first().cloned().ok_or("no segment")?;
    let mut others = MEMBERS.iter().copied().filter(|id| *id != a);
    let (b, c) = (others.next().ok_or("no b")?, others.next().ok_or("no c")?);
    drop(nodes.remove(&c));
    // About 400 KiB of Flows: B's snapshot, which A fetches and keeps, and
    // which C installs, spans several frames.
    heavy(nodes.get(&a).ok_or("no a")?, "h", 50)?;
    trim(&path, (a, nodes.get(&a).ok_or("no a")?), &first)?;
    nodes.insert(c, open(&path, c, &peers)?);
    snapshotted(&path, c)?;
    drop(nodes.remove(&b));
    let led = serving(&nodes, "probe-b")?;
    assert_states(nodes.get(&led).ok_or("no leader")?, "h", 50, &[])?;
    Ok(())
}
