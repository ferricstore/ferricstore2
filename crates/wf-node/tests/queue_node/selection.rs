//! Claim selection across shards: limits, due time, state, priority and
//! partition filters.

use std::collections::BTreeSet;

use wf_application::commands::queue::{
    CreateFlows, NewFlow, Partition, QueueService, shard_for_slot,
};

use super::{TestResult, call, claim, claim_request, create, directory, open};

#[test]
fn claims_across_sixteen_shards_return_every_flow_exactly_once() -> TestResult {
    let directory = directory("many")?;
    let node = open(&directory, 16)?;
    let ids: Vec<String> = (0..1_000).map(|index| format!("flow-{index}")).collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    for chunk in id_refs.chunks(250) {
        assert!(create(&node, chunk)?.iter().all(Result::is_ok));
    }
    let mut seen = BTreeSet::new();
    for _ in 0..20 {
        let jobs = claim(&node, 300)?;
        assert!(jobs.len() <= 300);
        for job in jobs {
            assert!(seen.insert(job.id), "a flow was claimed twice");
        }
        if seen.len() == ids.len() {
            break;
        }
    }
    assert_eq!(seen.len(), ids.len());
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn future_flows_other_states_and_lower_priorities_wait() -> TestResult {
    let directory = directory("filters")?;
    let node = open(&directory, 4)?;
    let future = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )? + 3_600_000;
    for (id, state, run_at_ms, priority) in [
        ("later", "queued", Some(future), 0),
        ("other", "waiting", None, 0),
        ("low", "queued", None, 0),
        ("high", "queued", None, 2),
    ] {
        let request = CreateFlows {
            flow_type: b"jobs".to_vec(),
            state: state.as_bytes().to_vec(),
            run_at_ms,
            priority,
            flows: vec![NewFlow {
                id: id.as_bytes().to_vec(),
                partition: Some(b"p".to_vec()),
                payload: None,
            }],
        };
        assert_eq!(call(|reply| node.create(request, reply))??, vec![Ok(())]);
    }
    let first = claim(&node, 1)?;
    assert_eq!(
        first
            .iter()
            .map(|job| job.id.as_slice())
            .collect::<Vec<_>>(),
        vec![b"high".as_slice()]
    );
    assert_eq!(
        first.first().map(|job| job.partition.as_slice()),
        Some(b"p".as_slice())
    );
    let rest = claim(&node, 10)?;
    assert_eq!(
        rest.iter().map(|job| job.id.as_slice()).collect::<Vec<_>>(),
        vec![b"low".as_slice()]
    );
    let mut waiting = claim_request(10);
    waiting.state = Some(b"waiting".to_vec());
    let other = call(|reply| node.claim(waiting, reply))??;
    assert_eq!(
        other
            .iter()
            .map(|job| job.id.as_slice())
            .collect::<Vec<_>>(),
        vec![b"other".as_slice()]
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_partition_filter_claims_only_that_partition() -> TestResult {
    let directory = directory("partition")?;
    let node = open(&directory, 8)?;
    let request = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: None,
        priority: 0,
        flows: ["a", "b", "c"]
            .iter()
            .zip([&b"p1"[..], b"p2", b"p1"])
            .map(|(id, partition)| NewFlow {
                id: id.as_bytes().to_vec(),
                partition: Some(partition.to_vec()),
                payload: None,
            })
            .collect(),
    };
    assert!(
        call(|reply| node.create(request, reply))??
            .iter()
            .all(Result::is_ok)
    );
    let mut only = claim_request(10);
    only.partitions = Some(vec![b"p1".to_vec()]);
    let jobs = call(|reply| node.claim(only, reply))??;
    let mut ids: Vec<&[u8]> = jobs.iter().map(|job| job.id.as_slice()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![b"a".as_slice(), b"c".as_slice()]);
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

/// A claim for one Flow finds a due Flow on any shard, even when the shard
/// the weighted split asks first holds only Flows scheduled later (ADR-013).
#[test]
fn a_one_flow_claim_finds_a_due_flow_behind_later_flows() -> TestResult {
    let directory = directory("shortfall")?;
    let node = open(&directory, 16)?;
    let later = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )? + 3_600_000;
    let flows: Vec<NewFlow> = (0..200)
        .map(|index| NewFlow {
            id: format!("later-{index}").into_bytes(),
            partition: None,
            payload: None,
        })
        .collect();
    let request = CreateFlows {
        flow_type: b"jobs".to_vec(),
        state: b"queued".to_vec(),
        run_at_ms: Some(later),
        priority: 0,
        flows,
    };
    assert!(
        call(|reply| node.create(request, reply))??
            .iter()
            .all(Result::is_ok)
    );
    let due = (0..100)
        .map(|index| format!("due-{index}"))
        .find(|id| {
            let partition = Partition::for_new_flow(id.as_bytes(), None);
            shard_for_slot(partition.slot(), 16) != Some(0)
        })
        .ok_or("every candidate id is on shard 0")?;
    assert!(create(&node, &[due.as_str()])?.iter().all(Result::is_ok));
    let jobs = claim(&node, 1)?;
    assert_eq!(
        jobs.iter().map(|job| job.id.as_slice()).collect::<Vec<_>>(),
        vec![due.as_bytes()]
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
