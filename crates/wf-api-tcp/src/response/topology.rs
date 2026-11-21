//! The `SHARDS` reply (0x0007): the slot table the SDK's topology-aware
//! client routes by, in Elixir's shape. This node leads every range: one
//! range of contiguous slots per shard, on lane `shard + 1`.

use wf_application::commands::queue::{SLOTS, shard_for_slot};

use crate::codec::Value;

use super::handshake::{int, map, text};
use super::{ReplyTooLong, STATUS_OK};

/// The node the table names.
#[derive(Clone, Copy, Debug)]
pub struct Topology<'a> {
    /// Node name.
    pub node: &'a str,
    /// Advertised host.
    pub host: &'a str,
    /// Native protocol port.
    pub port: u16,
    /// Number of shards.
    pub shard_count: u16,
}

/// Each shard's first and last slot.
fn ranges(shard_count: u16) -> Vec<(u16, u16, u16)> {
    let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
    for slot in 0..SLOTS {
        let Some(shard) = shard_for_slot(slot, shard_count) else {
            continue;
        };
        match ranges.last_mut() {
            Some((owner, _, last)) if *owner == shard => *last = slot,
            _ => ranges.push((shard, slot, slot)),
        }
    }
    ranges
}

/// The `SHARDS` reply body.
///
/// # Errors
/// Refuses names longer than `u32::MAX` bytes.
pub fn shards(topology: &Topology<'_>) -> Result<Vec<u8>, ReplyTooLong> {
    let port = int(u64::from(topology.port));
    let endpoint = map(vec![
        ("host", text(topology.host)),
        ("native_port", port.clone()),
        ("node", text(topology.node)),
    ]);
    let ranges = ranges(topology.shard_count)
        .into_iter()
        .map(|(shard, first, last)| {
            map(vec![
                ("endpoint", endpoint.clone()),
                ("first_slot", int(u64::from(first))),
                ("hint", text("leader")),
                ("lane_id", int(u64::from(shard).saturating_add(1))),
                ("last_slot", int(u64::from(last))),
                ("leader_node", text(topology.node)),
                ("native_host", text(topology.host)),
                ("native_port", port.clone()),
                ("owner_node", text(topology.node)),
                ("shard", int(u64::from(shard))),
            ])
        })
        .collect();
    let reply = map(vec![
        ("ranges", Value::Arr(ranges)),
        // The handshake's epoch: one node's table never changes while it runs.
        ("route_epoch", int(1)),
        ("shard_count", int(u64::from(topology.shard_count))),
        ("slots", int(u64::from(SLOTS))),
    ]);
    let mut body = STATUS_OK.to_be_bytes().to_vec();
    reply.encode(&mut body)?;
    Ok(body)
}
