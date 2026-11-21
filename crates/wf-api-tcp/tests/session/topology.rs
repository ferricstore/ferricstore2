//! `SHARDS` (0x0007): the slot table the SDK's topology-aware client routes
//! by, in Elixir's shape. One node leads every range: one range per shard,
//! contiguous slots, lane `shard + 1`.

use super::typed::{Typed, bin, map, reply};
use super::{Client, TestResult, start};

const SHARDS: u16 = 0x0007;

fn range(shard: i64, first: i64, last: i64, port: u16) -> Typed {
    let endpoint = map(&[
        ("host", bin("127.0.0.1")),
        ("native_port", Typed::Int(i64::from(port))),
        ("node", bin("test-node")),
    ]);
    map(&[
        ("endpoint", endpoint),
        ("first_slot", Typed::Int(first)),
        ("hint", bin("leader")),
        ("lane_id", Typed::Int(shard + 1)),
        ("last_slot", Typed::Int(last)),
        ("leader_node", bin("test-node")),
        ("native_host", bin("127.0.0.1")),
        ("native_port", Typed::Int(i64::from(port))),
        ("owner_node", bin("test-node")),
        ("shard", Typed::Int(shard)),
    ])
}

#[test]
fn shards_lists_one_leader_range_per_shard() -> TestResult {
    let running = start()?;
    let port = running.address.port();
    let mut client = Client::connect(&running)?;
    let shards = client.call(SHARDS, 1, 0, &[])?;
    let ranges = (0..4)
        .map(|shard| range(shard, shard * 256, shard * 256 + 255, port))
        .collect();
    let expected = map(&[
        ("ranges", Typed::List(ranges)),
        ("route_epoch", Typed::Int(1)),
        ("shard_count", Typed::Int(4)),
        ("slots", Typed::Int(1024)),
    ]);
    assert_eq!(reply(&shards.body)?, (0, expected));
    Ok(())
}

const CREATE: u16 = 0x0201;

#[test]
fn a_member_that_does_not_lead_answers_reroute() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let body = super::typed::encode(&map(&[
        ("id", bin("reroute1")),
        ("type", bin("jobs")),
        ("state", bin("queued")),
    ]))?;
    let refused = client.call(CREATE, 1, 0, &body)?;
    let expected = map(&[
        ("code", bin("reroute")),
        (
            "message",
            bin("REROUTE this member does not lead the slot; refresh SHARDS"),
        ),
        ("reason", bin("not_leader")),
        ("retry_after_ms", Typed::Int(0)),
        ("retryable", Typed::Bool(true)),
        ("safe_to_retry", Typed::Bool(true)),
    ]);
    assert_eq!(reply(&refused.body)?, (5, expected));
    Ok(())
}

/// A group member's service: only its route matters here.
struct Grouped(Option<u64>);

impl wf_application::commands::queue::QueueService for Grouped {
    fn create(
        &self,
        _request: wf_application::commands::queue::CreateFlows,
        reply: wf_application::commands::queue::Reply<
            Vec<wf_application::commands::queue::ItemOutcome>,
        >,
    ) {
        reply(Err(wf_application::commands::queue::ServiceError::Reroute));
    }

    fn claim(
        &self,
        _request: wf_application::commands::queue::ClaimFlows,
        reply: wf_application::commands::queue::Reply<
            Vec<wf_application::commands::queue::ClaimedFlow>,
        >,
    ) {
        reply(Err(wf_application::commands::queue::ServiceError::Reroute));
    }

    fn complete(
        &self,
        _completions: Vec<wf_application::commands::queue::CompleteFlow>,
        reply: wf_application::commands::queue::Reply<
            Vec<wf_application::commands::queue::ItemOutcome>,
        >,
    ) {
        reply(Err(wf_application::commands::queue::ServiceError::Reroute));
    }

    fn route(&self) -> Option<wf_application::commands::queue::Route> {
        Some(wf_application::commands::queue::Route {
            leader: self.0,
            term: 7,
        })
    }
}

impl wf_application::commands::flow::FlowService for Grouped {
    fn execute(
        &self,
        _command: wf_application::commands::flow::FlowCommand,
        reply: wf_application::commands::queue::Reply<wf_application::commands::flow::Term>,
    ) {
        reply(Err(wf_application::commands::queue::ServiceError::Reroute));
    }
}

#[test]
fn a_group_member_names_its_group_s_leader() -> TestResult {
    for (leader, hint, host, port) in [
        (Some(2), "leader", "10.0.0.2", 7002),
        (None, "leader_unknown", "127.0.0.1", 0),
    ] {
        let running = super::serving_service(std::sync::Arc::new(Grouped(leader)))?;
        let port = if port == 0 {
            running.address.port()
        } else {
            port
        };
        let node = if leader.is_some() {
            "ferricstore2-2"
        } else {
            "test-node"
        };
        let mut client = Client::connect(&running)?;
        let shards = client.call(SHARDS, 1, 0, &[])?;
        let (status, value) = reply(&shards.body)?;
        assert_eq!(status, 0);
        assert_eq!(super::typed::at(&value, &["route_epoch"])?, &Typed::Int(7));
        let Typed::List(ranges) = super::typed::at(&value, &["ranges"])? else {
            return Err("no ranges".into());
        };
        let first = ranges.first().ok_or("no range")?;
        assert_eq!(
            super::typed::at(first, &["hint"])?,
            &bin(hint),
            "{leader:?}"
        );
        assert_eq!(super::typed::at(first, &["leader_node"])?, &bin(node));
        let endpoint = super::typed::at(first, &["endpoint"])?;
        assert_eq!(super::typed::at(endpoint, &["host"])?, &bin(host));
        assert_eq!(
            super::typed::at(endpoint, &["native_port"])?,
            &Typed::Int(i64::from(port))
        );
    }
    Ok(())
}
