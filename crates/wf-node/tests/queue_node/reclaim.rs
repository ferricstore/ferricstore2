//! Expired leases are reclaimed in Elixir's order (ADR-013): a new lease and
//! the next fence, the logical run state kept, the old lease stale.

use std::time::Duration;

use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, QueueService};

use super::{
    TestResult, call, claim, claim_request, complete, completion, create, directory, open,
};

fn short_lease(limit: usize) -> ClaimFlows {
    ClaimFlows {
        lease_ms: 50,
        ..claim_request(limit)
    }
}

fn reclaiming(limit: usize, ratio: u8) -> ClaimFlows {
    ClaimFlows {
        reclaim: Some(ratio),
        ..claim_request(limit)
    }
}

fn ids(jobs: &[ClaimedFlow]) -> Vec<&[u8]> {
    jobs.iter().map(|job| job.id.as_slice()).collect()
}

fn wait_for_expiry() {
    std::thread::sleep(Duration::from_millis(150));
}

#[test]
fn an_expired_lease_is_reclaimed_with_the_next_fence() -> TestResult {
    let directory = directory("reclaim")?;
    let node = open(&directory, 4)?;
    assert!(create(&node, &["a"])?.iter().all(Result::is_ok));
    let first = call(|reply| node.claim(short_lease(1), reply))??;
    let old = first.first().ok_or("no claim")?.clone();
    let early = call(|reply| node.claim(reclaiming(1, 25), reply))??;
    assert!(early.is_empty(), "a live lease was reclaimed");
    wait_for_expiry();
    assert!(claim(&node, 1)?.is_empty(), "reclaimed without asking");
    let again = call(|reply| node.claim(reclaiming(1, 25), reply))??;
    let new = again.first().ok_or("no reclaim")?.clone();
    assert_eq!(new.id, old.id);
    assert_eq!(new.partition, old.partition);
    assert_eq!(new.fence, old.fence + 1);
    assert_ne!(new.lease_token, old.lease_token);
    assert_eq!(new.run_state, b"queued");
    let stale = complete(&node, vec![completion(&old)])?;
    let message = stale
        .first()
        .and_then(|outcome| outcome.as_ref().err())
        .map(ToString::to_string);
    assert_eq!(message.as_deref(), Some("ERR stale flow lease"));
    assert_eq!(complete(&node, vec![completion(&new)])?, vec![Ok(())]);
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn reclaim_comes_first_then_due_flows_then_more_reclaim() -> TestResult {
    let directory = directory("reclaim-order")?;
    let node = open(&directory, 1)?;
    assert!(create(&node, &["a", "b"])?.iter().all(Result::is_ok));
    assert_eq!(call(|reply| node.claim(short_lease(2), reply))??.len(), 2);
    assert!(create(&node, &["c", "d"])?.iter().all(Result::is_ok));
    wait_for_expiry();
    let jobs = call(|reply| node.claim(reclaiming(4, 25), reply))??;
    assert_eq!(
        ids(&jobs),
        vec![
            b"a".as_slice(),
            b"c".as_slice(),
            b"d".as_slice(),
            b"b".as_slice()
        ]
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn one_flow_prefers_a_due_flow_and_running_state_takes_only_expired() -> TestResult {
    let directory = directory("reclaim-one")?;
    let node = open(&directory, 1)?;
    assert!(create(&node, &["a"])?.iter().all(Result::is_ok));
    assert_eq!(call(|reply| node.claim(short_lease(1), reply))??.len(), 1);
    assert!(create(&node, &["b"])?.iter().all(Result::is_ok));
    wait_for_expiry();
    let running = ClaimFlows {
        states: vec![b"running".to_vec()],
        ..claim_request(5)
    };
    let preferred = call(|reply| node.claim(reclaiming(1, 25), reply))??;
    assert_eq!(ids(&preferred), vec![b"b".as_slice()]);
    let expired = call(|reply| node.claim(running, reply))??;
    assert_eq!(ids(&expired), vec![b"a".as_slice()]);
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_reclaim_survives_a_restart() -> TestResult {
    let directory = directory("reclaim-restart")?;
    let (old, new) = {
        let node = open(&directory, 4)?;
        assert!(create(&node, &["a"])?.iter().all(Result::is_ok));
        let first = call(|reply| node.claim(short_lease(1), reply))??;
        wait_for_expiry();
        let again = call(|reply| node.claim(reclaiming(1, 25), reply))??;
        (
            first.first().ok_or("no claim")?.clone(),
            again.first().ok_or("no reclaim")?.clone(),
        )
    };
    let node = open(&directory, 3)?;
    assert!(
        complete(&node, vec![completion(&old)])?
            .iter()
            .all(Result::is_err)
    );
    assert_eq!(complete(&node, vec![completion(&new)])?, vec![Ok(())]);
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
