//! Blocking claims (ADR-013) wait for a create, a due time, an expiring lease
//! or their deadline; a cancelled or stopped wait takes nothing.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wf_application::commands::queue::{
    Cancellation, ClaimFlows, ClaimedFlow, CreateFlows, QueueService, ServiceError,
};
use wf_node::composition::QueueNode;

use super::{TestResult, call, claim, claim_request, create, directory, flow, open};

type Claimed = Result<Vec<ClaimedFlow>, ServiceError>;

fn blocking(limit: usize, block_ms: u64) -> ClaimFlows {
    ClaimFlows {
        block_ms: Some(block_ms),
        ..claim_request(limit)
    }
}

/// Starts a claim and returns where its reply arrives.
fn start(node: &QueueNode, request: ClaimFlows) -> Receiver<(Claimed, Instant)> {
    let (sender, receiver) = mpsc::channel();
    node.claim(
        request,
        Box::new(move |result| {
            // The test may have finished and dropped the receiver.
            let _delivered = sender.send((result, Instant::now()));
        }),
    );
    receiver
}

fn ids(jobs: &[ClaimedFlow]) -> Vec<&[u8]> {
    jobs.iter().map(|job| job.id.as_slice()).collect()
}

fn wall_ms() -> TestResult<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

const PATIENCE: Duration = Duration::from_secs(5);

#[test]
fn a_blocking_claim_wakes_for_a_create() -> TestResult {
    let directory = directory("block-create")?;
    let node = open(&directory, 4)?;
    let started = Instant::now();
    let waiting = start(&node, blocking(1, 10_000));
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        waiting.try_recv().is_err(),
        "returned before any Flow existed"
    );
    assert!(create(&node, &["late"])?.iter().all(Result::is_ok));
    let (result, at) = waiting.recv_timeout(PATIENCE)?;
    assert_eq!(ids(&result?), vec![b"late".as_slice()]);
    assert!(at.duration_since(started) < Duration::from_secs(3));
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_blocking_claim_returns_nothing_at_its_deadline() -> TestResult {
    let directory = directory("block-deadline")?;
    let node = open(&directory, 4)?;
    let started = Instant::now();
    let (result, at) = start(&node, blocking(3, 250)).recv_timeout(PATIENCE)?;
    assert_eq!(result?, Vec::new());
    let waited = at.duration_since(started);
    assert!(waited >= Duration::from_millis(240), "{waited:?}");
    assert!(waited < Duration::from_secs(2), "{waited:?}");
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_blocking_claim_wakes_when_a_scheduled_flow_falls_due() -> TestResult {
    let directory = directory("block-due")?;
    let node = open(&directory, 4)?;
    let request = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: Some(wall_ms()? + 400),
        priority: 0,
        flows: vec![flow("soon")],
    };
    assert_eq!(call(|reply| node.create(request, reply))??, vec![Ok(())]);
    let started = Instant::now();
    let (result, at) = start(&node, blocking(1, 10_000)).recv_timeout(PATIENCE)?;
    assert_eq!(ids(&result?), vec![b"soon".as_slice()]);
    let waited = at.duration_since(started);
    assert!(waited >= Duration::from_millis(300), "{waited:?}");
    assert!(waited < Duration::from_secs(3), "{waited:?}");
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_blocking_reclaim_wakes_when_a_lease_expires() -> TestResult {
    let directory = directory("block-lease")?;
    let node = open(&directory, 4)?;
    assert!(create(&node, &["held"])?.iter().all(Result::is_ok));
    let short = ClaimFlows {
        lease_ms: 300,
        ..claim_request(1)
    };
    let first = call(|reply| node.claim(short, reply))??;
    let fence = first.first().ok_or("no claim")?.fence;
    let reclaim = ClaimFlows {
        reclaim: Some(25),
        ..blocking(1, 10_000)
    };
    let (result, _) = start(&node, reclaim).recv_timeout(PATIENCE)?;
    let jobs = result?;
    assert_eq!(ids(&jobs), vec![b"held".as_slice()]);
    assert_eq!(jobs.first().map(|job| job.fence), Some(fence + 1));
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn block_zero_waits_until_a_flow_arrives() -> TestResult {
    let directory = directory("block-forever")?;
    let node = open(&directory, 2)?;
    let waiting = start(&node, blocking(1, 0));
    std::thread::sleep(Duration::from_millis(500));
    assert!(waiting.try_recv().is_err(), "a forever claim returned");
    assert!(create(&node, &["x"])?.iter().all(Result::is_ok));
    let (result, _) = waiting.recv_timeout(PATIENCE)?;
    assert_eq!(ids(&result?), vec![b"x".as_slice()]);
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_cancelled_blocking_claim_takes_nothing() -> TestResult {
    let directory = directory("block-cancel")?;
    let node = open(&directory, 2)?;
    let cancellation = Cancellation::new();
    let request = ClaimFlows {
        cancellation: Some(cancellation.clone()),
        ..blocking(1, 0)
    };
    let waiting = start(&node, request);
    std::thread::sleep(Duration::from_millis(100));
    cancellation.cancel();
    let (result, _) = waiting.recv_timeout(PATIENCE)?;
    assert_eq!(result?, Vec::new());
    assert!(create(&node, &["kept"])?.iter().all(Result::is_ok));
    assert_eq!(ids(&claim(&node, 5)?), vec![b"kept".as_slice()]);
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stopping_the_node_fails_a_parked_claim() -> TestResult {
    let directory = directory("block-stop")?;
    let node = open(&directory, 2)?;
    let waiting = start(&node, blocking(1, 0));
    std::thread::sleep(Duration::from_millis(100));
    drop(node);
    let (result, _) = waiting.recv_timeout(PATIENCE)?;
    assert_eq!(result, Err(ServiceError::Unavailable));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
