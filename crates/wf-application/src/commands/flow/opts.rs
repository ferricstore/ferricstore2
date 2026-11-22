//! Reading request options as the Elixir server does (ADR-014).
//!
//! Each reader matches one `Ferricstore.Flow.MutationAttrs` helper: the same
//! defaults, the same accepted values and the same `ERR flow <key> ...` text.
//! An explicit nil is a present value, as in an Elixir keyword list.

use super::FlowError;
use super::term::Term;

/// Elixir's native `@known_flow_options`, sorted: the option names any Flow
/// command may carry (a command ignores the ones it does not read).
const KNOWN_OPTIONS: [&str; 156] = [
    "actual_amount",
    "after_ms",
    "amount",
    "approver",
    "assignees",
    "at_ms",
    "attempt",
    "attribute",
    "attributes",
    "attributes_delete",
    "attributes_merge",
    "backoff",
    "base_ms",
    "block_ms",
    "catchup_policy",
    "children",
    "config_version",
    "consistent_projection",
    "correlation_id",
    "count",
    "cron",
    "delay_ms",
    "drop_values",
    "due_after_ms",
    "effect_key",
    "effect_type",
    "end_at_ms",
    "error",
    "error_class",
    "error_classes",
    "error_ref",
    "event",
    "every_ms",
    "exhaust_to",
    "exhausted_to",
    "expect_state",
    "expected_generation",
    "expires_at_ms",
    "external_id",
    "failure",
    "failure_rate_pct",
    "failure_threshold",
    "fencing_token",
    "fire_at_ms",
    "flow_id",
    "from_event",
    "from_ms",
    "from_state",
    "from_version",
    "full",
    "governance_limit_scope",
    "governance_scope",
    "governance_shard_id",
    "group_id",
    "half_open_max_probes",
    "half_open_success_threshold",
    "history_hot_max_events",
    "history_max_events",
    "id",
    "id_prefix",
    "idempotency_key",
    "idempotent",
    "if_state",
    "include_cold",
    "independent",
    "indexed_attributes",
    "indexed_state_meta",
    "initial_state",
    "items",
    "jitter_pct",
    "kind",
    "latency_ms",
    "latency_threshold_ms",
    "lease_ms",
    "lease_token",
    "limit",
    "local_cache",
    "max_active_ms",
    "max_attempts",
    "max_bytes",
    "max_fires",
    "max_ms",
    "max_retries",
    "min_calls",
    "name",
    "now_ms",
    "older_than_ms",
    "on_child_failed",
    "on_parent_closed",
    "open_ms",
    "operation_digest",
    "overlap_policy",
    "overlap_retry_ms",
    "override",
    "override_values",
    "overwrite",
    "owner_flow_id",
    "parent_flow_id",
    "partition_key",
    "partition_keys",
    "payload",
    "payload_max_bytes",
    "payload_ref",
    "payload_refs",
    "policy_hash",
    "policy_version",
    "priority",
    "reason",
    "reason_ref",
    "reclaim_expired",
    "reclaim_ratio",
    "replace",
    "requested_by",
    "reservation_id",
    "reservation_ids",
    "result",
    "result_ref",
    "retention_ttl_ms",
    "retry",
    "retry_at_ms",
    "return",
    "rev",
    "root_flow_id",
    "run_at_ms",
    "scope",
    "shard_id",
    "signal",
    "start_at_ms",
    "state",
    "state_meta",
    "states",
    "status",
    "steps",
    "success",
    "target",
    "target_type",
    "terminal_local_only",
    "terminal_only",
    "timeout_ms",
    "timezone",
    "to_event",
    "to_ms",
    "to_state",
    "to_version",
    "transition_to",
    "ttl_ms",
    "type",
    "usage",
    "value",
    "value_max_bytes",
    "value_refs",
    "values",
    "wait",
    "wait_state",
    "window_ms",
    "worker",
];

/// Largest exact integer the Elixir server accepts for times and counts.
pub const MAX_EXACT: u64 = 9_007_199_254_740_991;

/// The options of one request.
pub struct Opts<'a> {
    entries: &'a [(Vec<u8>, Term)],
}

impl<'a> Opts<'a> {
    /// Reads a request map as Elixir's native server does (audit gap G35):
    /// a key outside `allowed` that is one of Elixir's known Flow options
    /// is ignored, except `return`, which only commands taking it accept;
    /// any other key is refused.
    pub fn new(command: &str, args: &'a Term, allowed: &[&str]) -> Result<Self, FlowError> {
        let Term::Map(entries) = args else {
            return Err(FlowError::rejected(format!(
                "ERR native {command} payload must be a map"
            )));
        };
        for (key, _) in entries {
            if allowed.iter().any(|name| name.as_bytes() == key.as_slice()) {
                continue;
            }
            if key.as_slice() == b"return" {
                return Err(FlowError::rejected(
                    "ERR flow return option is not supported".to_owned(),
                ));
            }
            if KNOWN_OPTIONS
                .binary_search_by(|known| known.as_bytes().cmp(key.as_slice()))
                .is_err()
            {
                return Err(FlowError::rejected(format!(
                    "ERR native unknown flow option {}",
                    String::from_utf8_lossy(key)
                )));
            }
        }
        Ok(Self { entries })
    }

    /// The value under `key`, nil included.
    pub fn get(&self, key: &str) -> Option<&'a Term> {
        self.entries
            .iter()
            .find(|(name, _)| name.as_slice() == key.as_bytes())
            .map(|(_, value)| value)
    }

    /// Whether `key` was sent at all.
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// `required_binary/2`: a non-empty binary.
    pub fn required_binary(&self, key: &str) -> Result<Vec<u8>, FlowError> {
        match self.get(key) {
            Some(Term::Bin(value)) if !value.is_empty() => Ok(value.clone()),
            Some(_) => Err(FlowError::flow(key, "must be a non-empty string")),
            None => Err(FlowError::flow(key, "is required")),
        }
    }

    /// `optional_binary/3`: a non-empty binary, `default` when absent.
    pub fn optional_binary(&self, key: &str, default: &[u8]) -> Result<Vec<u8>, FlowError> {
        match self.get(key) {
            None => Ok(default.to_vec()),
            Some(Term::Bin(value)) if !value.is_empty() => Ok(value.clone()),
            Some(_) => Err(FlowError::flow(key, "must be a non-empty string")),
        }
    }

    /// `optional_binary_or_nil/3` with a nil default.
    pub fn binary_or_nil(&self, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
        match self.get(key) {
            None | Some(Term::Nil) => Ok(None),
            Some(Term::Bin(value)) => Ok(Some(value.clone())),
            Some(_) => Err(FlowError::flow(key, "must be a string")),
        }
    }

    /// `optional_boolean/3`.
    pub fn boolean(&self, key: &str, default: bool) -> Result<bool, FlowError> {
        match self.get(key) {
            None => Ok(default),
            Some(Term::Bool(value)) => Ok(*value),
            Some(_) => Err(FlowError::flow(key, "must be a boolean")),
        }
    }

    /// `optional_non_neg_integer/3`: 0 through [`MAX_EXACT`].
    pub fn non_neg_integer(&self, key: &str) -> Result<Option<u64>, FlowError> {
        match self.get(key) {
            None => Ok(None),
            Some(Term::Int(value)) => match u64::try_from(*value) {
                Ok(value) if value <= MAX_EXACT => Ok(Some(value)),
                Ok(_) => Err(FlowError::flow(
                    key,
                    &format!("exceeds maximum {MAX_EXACT}"),
                )),
                Err(_) => Err(FlowError::flow(key, "must be a non-negative integer")),
            },
            Some(_) => Err(FlowError::flow(key, "must be a non-negative integer")),
        }
    }

    /// `optional_pos_integer`: 1 through [`MAX_EXACT`].
    pub fn positive_integer(&self, key: &str) -> Result<Option<u64>, FlowError> {
        match self.get(key) {
            None => Ok(None),
            Some(Term::Int(value)) => match u64::try_from(*value) {
                Ok(0) | Err(_) => Err(FlowError::flow(key, "must be a positive integer")),
                Ok(value) if value <= MAX_EXACT => Ok(Some(value)),
                Ok(_) => Err(FlowError::flow(
                    key,
                    &format!("exceeds maximum {MAX_EXACT}"),
                )),
            },
            Some(_) => Err(FlowError::flow(key, "must be a positive integer")),
        }
    }

    /// `optional_priority/2`: 0 through 2.
    pub fn priority(&self, default: u8) -> Result<u8, FlowError> {
        match self.get("priority") {
            None => Ok(default),
            Some(Term::Int(value)) if (0..=2).contains(value) => {
                Ok(u8::try_from(*value).unwrap_or(default))
            }
            Some(_) => Err(FlowError::flow("priority", "must be between 0 and 2")),
        }
    }

    /// A list of non-empty binaries, such as value names to read.
    pub fn binary_list(&self, key: &str) -> Result<Vec<Vec<u8>>, FlowError> {
        match self.get(key) {
            None | Some(Term::Nil) => Ok(Vec::new()),
            Some(Term::List(items)) => items
                .iter()
                .map(|item| match item {
                    Term::Bin(name) if !name.is_empty() => Ok(name.clone()),
                    _ => Err(FlowError::flow(key, "must be a list of strings")),
                })
                .collect(),
            Some(Term::Bin(name)) if !name.is_empty() => Ok(vec![name.clone()]),
            Some(_) => Err(FlowError::flow(key, "must be a list of strings")),
        }
    }
}
