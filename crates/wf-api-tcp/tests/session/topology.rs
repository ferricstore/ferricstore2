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
