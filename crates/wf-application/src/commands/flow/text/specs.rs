//! `NativeAstParser`'s `@flow_option_specs`: each text option's name, the
//! typed-map key it fills and how its value is read.

use super::options::Kind::{Binary, Boolean, Infinity, NonNegative, Partition, Positive, Ref};
use super::options::Spec;

pub(super) const CREATE: Spec = &[
    ("TYPE", "type", Binary),
    ("STATE", "state", Binary),
    ("PAYLOAD", "payload", Binary),
    ("PAYLOAD_REF", "payload_ref", Ref("payload_ref")),
    ("PARENT_FLOW_ID", "parent_flow_id", Ref("parent_flow_id")),
    ("ROOT_FLOW_ID", "root_flow_id", Ref("root_flow_id")),
    ("CORRELATION_ID", "correlation_id", Ref("correlation_id")),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("NOW", "now_ms", NonNegative),
    ("PRIORITY", "priority", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("RETENTION_TTL", "retention_ttl_ms", Positive),
    ("RETENTION_TTL_MS", "retention_ttl_ms", Positive),
    ("MAX_ACTIVE_MS", "max_active_ms", Infinity),
    (
        "HISTORY_HOT_MAX_EVENTS",
        "history_hot_max_events",
        NonNegative,
    ),
    ("HISTORY_MAX_EVENTS", "history_max_events", Positive),
    ("IDEMPOTENT", "idempotent", Boolean),
    ("RETURN", "return", Binary),
];

pub(super) const VALUE_PUT: Spec = &[
    ("PARTITION", "partition_key", Partition),
    ("OWNER_FLOW_ID", "owner_flow_id", Ref("owner_flow_id")),
    ("TTL", "ttl_ms", Positive),
    ("TTL_MS", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
];

pub(super) const SIGNAL: Spec = &[
    ("PARTITION", "partition_key", Partition),
    ("SIGNAL", "signal", Binary),
    ("IDEMPOTENCY", "idempotency_key", Binary),
    ("IDEMPOTENCY_KEY", "idempotency_key", Binary),
    ("IF_STATE", "if_state", Binary),
    ("TRANSITION_TO", "transition_to", Binary),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("NOW", "now_ms", NonNegative),
];

pub(super) const POLICY_GET: Spec = &[("STATE", "state", Binary)];

pub(super) const CLAIM_DUE: Spec = &[
    ("WORKER", "worker", Binary),
    ("STATE", "state", Binary),
    ("LEASE_MS", "lease_ms", Positive),
    ("LIMIT", "limit", Positive),
    ("PRIORITY", "priority", NonNegative),
    ("NOW", "now_ms", NonNegative),
    ("BLOCK", "block_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("RETURN", "return", Binary),
    ("RECLAIM_EXPIRED", "reclaim_expired", Boolean),
    ("RECLAIM_RATIO", "reclaim_ratio", NonNegative),
];

pub(super) const TERMINAL: Spec = &[
    ("FENCING", "fencing_token", NonNegative),
    ("RESULT", "result", Binary),
    ("PAYLOAD", "payload", Binary),
    ("TTL", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
];

pub(super) const EXTEND_LEASE: Spec = &[
    ("FENCING", "fencing_token", NonNegative),
    ("LEASE_MS", "lease_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("RETURN", "return", Binary),
];

pub(super) const TRANSITION: Spec = &[
    ("FENCING", "fencing_token", NonNegative),
    ("LEASE_TOKEN", "lease_token", Binary),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("PRIORITY", "priority", NonNegative),
    ("PAYLOAD", "payload", Binary),
    ("PAYLOAD_REF", "payload_ref", Ref("payload_ref")),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
];

pub(super) const RETRY: Spec = &[
    ("FENCING", "fencing_token", NonNegative),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("ERROR", "error", Binary),
    ("PAYLOAD", "payload", Binary),
    ("MAX_RETRIES", "max_retries", NonNegative),
    ("EXHAUSTED_TO", "exhausted_to", Binary),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
];

pub(super) const FAIL: Spec = &[
    ("FENCING", "fencing_token", NonNegative),
    ("ERROR", "error", Binary),
    ("PAYLOAD", "payload", Binary),
    ("TTL", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
];

pub(super) const CANCEL: Spec = &[
    ("FENCING", "fencing_token", NonNegative),
    ("LEASE_TOKEN", "lease_token", Binary),
    ("REASON", "reason", Binary),
    ("REASON_REF", "reason_ref", Ref("reason_ref")),
    ("TTL", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
];

pub(super) const REWIND: Spec = &[
    ("TO_EVENT", "to_event", Binary),
    ("REASON", "reason", Binary),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("EXPECT_STATE", "expect_state", Binary),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
];

pub(super) const STATS: Spec = &[
    ("STATE", "state", Binary),
    ("COUNT", "count", Positive),
    ("PARTITION", "partition_key", Partition),
    ("INCLUDE_COLD", "include_cold", Boolean),
    ("CONSISTENT_PROJECTION", "consistent_projection", Boolean),
];

pub(super) const INDEX_QUERY: Spec = &[
    ("COUNT", "count", Positive),
    ("PARTITION", "partition_key", Partition),
    ("FROM_MS", "from_ms", NonNegative),
    ("TO_MS", "to_ms", NonNegative),
    ("REV", "rev", Boolean),
    ("STATE", "state", Binary),
    ("TERMINAL_ONLY", "terminal_only", Boolean),
    ("INCLUDE_COLD", "include_cold", Boolean),
    ("CONSISTENT_PROJECTION", "consistent_projection", Boolean),
];

pub(super) const HISTORY: Spec = &[
    ("COUNT", "count", Positive),
    ("PARTITION", "partition_key", Partition),
    ("FROM_EVENT", "from_event", Binary),
    ("TO_EVENT", "to_event", Binary),
    ("FROM_MS", "from_ms", NonNegative),
    ("TO_MS", "to_ms", NonNegative),
    ("FROM_VERSION", "from_version", NonNegative),
    ("TO_VERSION", "to_version", NonNegative),
    ("REV", "rev", Boolean),
    ("EVENT", "event", Binary),
    ("WORKER", "worker", Binary),
    ("INCLUDE_COLD", "include_cold", Boolean),
    ("CONSISTENT_PROJECTION", "consistent_projection", Boolean),
    ("VALUES", "values", Boolean),
    ("PAYLOAD_MAX_BYTES", "payload_max_bytes", NonNegative),
    ("MAXBYTES", "payload_max_bytes", NonNegative),
];

pub(super) const RETENTION_CLEANUP: Spec =
    &[("LIMIT", "limit", Positive), ("NOW", "now_ms", NonNegative)];

pub(super) const PARTITION: Spec = &[
    ("PARTITION", "partition_key", Partition),
    ("INCLUDE_COLD", "include_cold", Boolean),
    ("CONSISTENT_PROJECTION", "consistent_projection", Boolean),
];

/// `spec(:create)` with `INDEPENDENT`, as `parse_flow_create_many/1`.
pub(super) const CREATE_MANY: Spec = &[
    ("TYPE", "type", Binary),
    ("STATE", "state", Binary),
    ("PAYLOAD", "payload", Binary),
    ("PAYLOAD_REF", "payload_ref", Ref("payload_ref")),
    ("PARENT_FLOW_ID", "parent_flow_id", Ref("parent_flow_id")),
    ("ROOT_FLOW_ID", "root_flow_id", Ref("root_flow_id")),
    ("CORRELATION_ID", "correlation_id", Ref("correlation_id")),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("NOW", "now_ms", NonNegative),
    ("PRIORITY", "priority", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("RETENTION_TTL", "retention_ttl_ms", Positive),
    ("RETENTION_TTL_MS", "retention_ttl_ms", Positive),
    ("MAX_ACTIVE_MS", "max_active_ms", Infinity),
    (
        "HISTORY_HOT_MAX_EVENTS",
        "history_hot_max_events",
        NonNegative,
    ),
    ("HISTORY_MAX_EVENTS", "history_max_events", Positive),
    ("IDEMPOTENT", "idempotent", Boolean),
    ("RETURN", "return", Binary),
    ("INDEPENDENT", "independent", Boolean),
];

pub(super) const MANY_TERMINAL: Spec = &[
    ("RESULT", "result", Binary),
    ("PAYLOAD", "payload", Binary),
    ("TTL", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("INDEPENDENT", "independent", Boolean),
    ("RETURN", "return", Binary),
];

pub(super) const MANY_RETRY: Spec = &[
    ("ERROR", "error", Binary),
    ("PAYLOAD", "payload", Binary),
    ("RUN_AT", "run_at_ms", NonNegative),
    ("MAX_RETRIES", "max_retries", NonNegative),
    ("EXHAUSTED_TO", "exhausted_to", Binary),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("INDEPENDENT", "independent", Boolean),
];

pub(super) const MANY_FAIL: Spec = &[
    ("ERROR", "error", Binary),
    ("PAYLOAD", "payload", Binary),
    ("TTL", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("INDEPENDENT", "independent", Boolean),
];

pub(super) const MANY_CANCEL: Spec = &[
    ("REASON", "reason", Binary),
    ("REASON_REF", "reason_ref", Ref("reason_ref")),
    ("TTL", "ttl_ms", Positive),
    ("NOW", "now_ms", NonNegative),
    ("PARTITION", "partition_key", Partition),
    ("INDEPENDENT", "independent", Boolean),
];

pub(super) const MANY_TRANSITION: Spec = &[
    ("RUN_AT", "run_at_ms", NonNegative),
    ("PRIORITY", "priority", NonNegative),
    ("PAYLOAD", "payload", Binary),
    ("PAYLOAD_REF", "payload_ref", Ref("payload_ref")),
    ("NOW", "now_ms", NonNegative),
    ("INDEPENDENT", "independent", Boolean),
];

pub(super) const SPAWN_CHILDREN: Spec = &[
    ("GROUP", "group_id", Binary),
    ("PARTITION", "partition_key", Partition),
    ("FENCING", "fencing_token", NonNegative),
    ("WAIT", "wait", Binary),
    ("ON_CHILD_FAILED", "on_child_failed", Binary),
    ("ON_PARENT_CLOSED", "on_parent_closed", Binary),
    ("SUCCESS", "success", Binary),
    ("FAILURE", "failure", Binary),
    ("FROM_STATE", "from_state", Binary),
    ("WAIT_STATE", "wait_state", Binary),
    ("LEASE_TOKEN", "lease_token", Binary),
    ("NOW", "now_ms", NonNegative),
    ("RETENTION_TTL", "retention_ttl_ms", Positive),
    ("RETENTION_TTL_MS", "retention_ttl_ms", Positive),
    ("MAX_ACTIVE_MS", "max_active_ms", Infinity),
    (
        "HISTORY_HOT_MAX_EVENTS",
        "history_hot_max_events",
        NonNegative,
    ),
    ("HISTORY_MAX_EVENTS", "history_max_events", Positive),
];
