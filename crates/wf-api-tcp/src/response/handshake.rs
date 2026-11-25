//! The `HELLO`, `STARTUP` and `OPTIONS` reply (ADR-011).
//!
//! The Python SDK refuses a server whose handshake lacks its 0.11.4 contract:
//! limits, compact codec opcodes, the `FLOW.POLICY.SET` and `FLOW.QUERY`
//! schemas and the Flow query manifest; the Go SDK also needs the protocol
//! versions. This server declares that contract so
//! the SDK connects, but implements only the queue commands; every other
//! command, including the declared query and policy commands, is refused
//! with an explicit error. Compact codecs are declared only for the replies
//! this server writes: claim jobs and the batch ok-list.

use crate::codec::Value;

use super::{ReplyTooLong, STATUS_OK};

/// Compact reply codecs this server writes, by name, with their opcodes.
pub const COMPACT_CODECS: [(&str, &[u16]); 2] = [
    ("flow_claim_jobs_v1", &[0x0203]),
    ("ok_list_v1", &[0x020F, 0x0210]),
];

const FLOW_QUERY_CAPABILITIES: [&str; 6] = [
    "flow_query_v1",
    "flow_query_result_projection_v1",
    "flow_explain_v1",
    "flow_explain_analyze_v1",
    "flow_composite_index_v1",
    "flow_query_index_status_v1",
];

const FLOW_QUERY_SHAPES: [&str; 12] = [
    "runs_by_run_id_record",
    "runs_by_partition_and_run_id_record",
    "runs_by_partition_predicates_ordered_records",
    "runs_by_partition_type_state_ordered_records",
    "runs_by_partition_type_terminals_ordered_records",
    "runs_by_partition_metadata_ordered_records",
    "runs_by_partition_type_running_lease_deadline_ordered_records",
    "runs_by_partition_parent_ordered_records",
    "runs_by_partition_root_ordered_records",
    "runs_by_partition_correlation_ordered_records",
    "runs_by_partition_predicates_count",
    "events_by_run_id_ordered_records",
];

/// What the handshake reply describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handshake<'a> {
    /// Node name.
    pub node: &'a str,
    /// Advertised host.
    pub host: &'a str,
    /// Native protocol port.
    pub port: u16,
    /// Number of shards.
    pub shard_count: u16,
    /// Connection-local client number.
    pub client_id: u64,
    /// Whether compact Flow replies were negotiated.
    pub compact: bool,
    /// Negotiated compact codecs, by name.
    pub selected: &'a [&'a str],
}

pub(super) fn text(value: &str) -> Value {
    Value::Bin(value.as_bytes().to_vec())
}

fn texts(values: &[&str]) -> Value {
    Value::Arr(values.iter().map(|value| text(value)).collect())
}

pub(super) fn map(entries: Vec<(&str, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

pub(super) fn int(value: u64) -> Value {
    i64::try_from(value).map_or(Value::Unsigned(value), Value::Int)
}

fn capabilities(handshake: &Handshake<'_>) -> Value {
    let opcodes = COMPACT_CODECS
        .iter()
        .map(|(name, opcodes)| {
            (
                *name,
                Value::Arr(opcodes.iter().map(|code| int(u64::from(*code))).collect()),
            )
        })
        .collect();
    let mut supported = vec!["typed_value"];
    supported.extend(COMPACT_CODECS.iter().map(|(name, _)| *name));
    map(vec![
        // The native protocol versions this server speaks; the Go SDK
        // refuses a server that does not list its own (audit gap G60).
        ("protocol_versions", Value::Arr(vec![int(1)])),
        // Requests may carry a deadline on the server's clock (audit gap
        // G62).
        (
            "deadlines",
            map(vec![
                ("clock", text("server_unix_ms")),
                ("field", text("deadline_ms")),
            ]),
        ),
        (
            "limits",
            map(vec![
                ("max_collection_response_items", int(10_000)),
                ("max_frame_bytes", int(16 * 1024 * 1024)),
                ("max_lane_queue", int(1024)),
                ("max_pipeline_commands", int(1024)),
                ("max_response_bytes", int(64 * 1024 * 1024)),
            ]),
        ),
        (
            "response_codecs",
            map(vec![
                ("compact_flow_responses", Value::Bool(handshake.compact)),
                ("compact_response_opcodes", map(opcodes)),
                ("selected_compact", texts(handshake.selected)),
                ("supported", texts(&supported)),
                ("typed_value", Value::Bool(true)),
            ]),
        ),
        (
            "schemas",
            map(vec![
                // Every field the policy decoder takes, as Elixir declares
                // them; the Elixir SDK checks each schema a server declares.
                (
                    "FLOW.POLICY.SET",
                    map(vec![
                        (
                            "fields",
                            texts(&[
                                "type",
                                "expected_generation",
                                "replace",
                                "max_active_ms",
                                "retry",
                                "retention",
                                "states",
                                "indexed_attributes",
                                "indexed_state_meta",
                                "version",
                                "governance",
                                "deadline_ms",
                            ]),
                        ),
                        ("required", texts(&["type"])),
                    ]),
                ),
                (
                    "FLOW.QUERY",
                    map(vec![
                        (
                            "fields",
                            texts(&["version", "query", "params", "deadline_ms"]),
                        ),
                        ("required", texts(&["version", "query"])),
                    ]),
                ),
            ]),
        ),
        (
            "flow_query",
            map(vec![
                ("capabilities", texts(&FLOW_QUERY_CAPABILITIES)),
                ("explain_contract", text("ferric.flow.explain/v1")),
                (
                    "index_status_contract",
                    text("ferric.flow.query.indexes/v1"),
                ),
                ("language_versions", texts(&["FQL1"])),
                ("request_contract", text("ferric.flow.query.request/v1")),
                ("result_contract", text("ferric.flow.query.result/v1")),
                ("shapes", texts(&FLOW_QUERY_SHAPES)),
            ]),
        ),
    ])
}

/// The handshake reply body.
///
/// # Errors
/// Refuses names longer than `u32::MAX` bytes.
pub fn handshake(handshake: &Handshake<'_>) -> Result<Vec<u8>, ReplyTooLong> {
    let endpoint = || {
        vec![
            ("host", text(handshake.host)),
            ("native_port", int(u64::from(handshake.port))),
            ("node", text(handshake.node)),
        ]
    };
    let mut route = endpoint();
    route.extend([
        ("endpoint", map(endpoint())),
        ("shard_count", int(u64::from(handshake.shard_count))),
        ("slots", int(1024)),
    ]);
    let reply = map(vec![
        ("auth_required", Value::Bool(false)),
        (
            "backpressure",
            map(vec![
                ("keydir_full", Value::Bool(false)),
                ("reject_writes", Value::Bool(false)),
                ("retry_after_ms", int(0)),
            ]),
        ),
        ("capabilities", capabilities(handshake)),
        ("client_id", int(handshake.client_id)),
        ("compression", text("none")),
        ("protocol", text("ferricstore-native")),
        ("route", map(route)),
        ("route_epoch", int(1)),
        (
            "server",
            map(vec![
                ("native_enabled", Value::Bool(true)),
                ("native_port", int(u64::from(handshake.port))),
                ("node", text(handshake.node)),
                ("protocol", text("ferric")),
            ]),
        ),
        ("version", int(1)),
    ]);
    let mut body = STATUS_OK.to_be_bytes().to_vec();
    reply.encode(&mut body)?;
    Ok(body)
}
