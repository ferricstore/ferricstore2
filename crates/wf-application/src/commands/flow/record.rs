//! What a Flow carries beyond its lifecycle record, and the record map the
//! Elixir server replies with (`Ferricstore.Flow.RecordProjection.public/1`).

use std::collections::BTreeMap;

use wf_domain::workflow::{WorkflowRecord, WorkflowStatus};
use wf_types::version::TimestampMillis;

use super::children::ChildGroup;
use super::create::MaxActive;
use super::keys;
use super::meta::{self, Attributes, StateMeta};
use super::term::{Term, map};
use crate::routing::Partition;

/// Elixir's default terminal retention: 7 days.
pub const DEFAULT_RETENTION_TTL_MS: u64 = 604_800_000;
/// Elixir's default history cap.
pub const DEFAULT_HISTORY_MAX_EVENTS: u64 = 100_000;

/// One named value of a Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedValue {
    /// Version, from 1, raised each time the value is replaced.
    pub version: u64,
    /// The stored bytes.
    pub bytes: Vec<u8>,
}

/// A Flow's fields beyond the lifecycle record.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowMeta {
    /// Parent Flow id.
    pub parent_flow_id: Option<Vec<u8>>,
    /// Parent partition.
    pub parent_partition_key: Option<Vec<u8>>,
    /// Root Flow id; the Flow itself when absent.
    pub root_flow_id: Option<Vec<u8>>,
    /// Correlation id.
    pub correlation_id: Option<Vec<u8>>,
    /// Terminal retention.
    pub retention_ttl_ms: u64,
    /// Active-time limit, if set.
    pub max_active_ms: Option<MaxActive>,
    /// Hot history cap.
    pub history_hot_max_events: u64,
    /// History cap.
    pub history_max_events: u64,
    /// Attributes.
    pub attributes: Attributes,
    /// Per-state metadata.
    pub state_meta: StateMeta,
    /// Named values, by name.
    pub values: BTreeMap<Vec<u8>, NamedValue>,
    /// Version of the payload, result and error references.
    pub payload_version: u64,
    /// Version of the result reference.
    pub result_version: u64,
    /// Version of the error reference.
    pub error_version: u64,
    /// Retention set by the terminal command, if any.
    pub ttl_ms: Option<u64>,
    /// When a terminal Flow is removed, set when it became terminal.
    pub terminal_until: Option<u64>,
    /// Signal idempotency keys and the digest of the content each applied.
    pub signal_keys: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Child groups by id.
    pub child_groups: BTreeMap<Vec<u8>, ChildGroup>,
}

impl Default for FlowMeta {
    fn default() -> Self {
        Self {
            parent_flow_id: None,
            parent_partition_key: None,
            root_flow_id: None,
            correlation_id: None,
            retention_ttl_ms: DEFAULT_RETENTION_TTL_MS,
            max_active_ms: None,
            history_hot_max_events: 0,
            history_max_events: DEFAULT_HISTORY_MAX_EVENTS,
            attributes: Attributes::new(),
            state_meta: StateMeta::new(),
            values: BTreeMap::new(),
            payload_version: 1,
            result_version: 1,
            error_version: 1,
            ttl_ms: None,
            terminal_until: None,
            signal_keys: BTreeMap::new(),
            child_groups: BTreeMap::new(),
        }
    }
}

/// Elixir's default `payload_max_bytes`: 64 KiB.
pub const DEFAULT_PAYLOAD_MAX_BYTES: u64 = 65_536;

/// Which named values a read includes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Values {
    /// None.
    #[default]
    None,
    /// These names, where the Flow has them.
    Names(Vec<Vec<u8>>),
    /// Every named value.
    All,
}

/// What a read asks to include besides the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Include {
    /// The payload, result and error bytes with their sizes; a value above
    /// `max_bytes` is reported as omitted with its size.
    pub payload: bool,
    /// Largest value returned inline.
    pub max_bytes: u64,
    /// Named values, as `values`.
    pub values: Values,
}

impl Default for Include {
    fn default() -> Self {
        Self {
            payload: false,
            max_bytes: DEFAULT_PAYLOAD_MAX_BYTES,
            values: Values::None,
        }
    }
}

/// A stored Flow as one reply map.
pub struct View<'a> {
    /// Lifecycle record.
    pub record: &'a WorkflowRecord,
    /// Partition.
    pub partition: &'a Partition,
    /// Claim priority.
    pub priority: u8,
    /// Extra fields.
    pub meta: &'a FlowMeta,
}

/// Elixir's `ValueHydration.apply_value_result`: the value and its size, or
/// its size and an omitted flag when it is above `max_bytes`.
fn hydrate(entries: &mut Vec<(&str, Term)>, kind: &str, value: Option<&[u8]>, max_bytes: u64) {
    let Some(value) = value else {
        return;
    };
    let size = u64::try_from(value.len()).unwrap_or(u64::MAX);
    let (field, size_field, omitted_field) = match kind {
        "payload" => ("payload", "payload_size", "payload_omitted"),
        "result" => ("result", "result_size", "result_omitted"),
        _ => ("error", "error_size", "error_omitted"),
    };
    if size > max_bytes {
        entries.push((omitted_field, Term::Bool(true)));
    } else {
        entries.push((field, Term::Bin(value.to_vec())));
    }
    entries.push((size_field, Term::int(size)));
}

fn ms(value: Option<TimestampMillis>) -> Term {
    Term::optional_int(value.map(TimestampMillis::get))
}

impl View<'_> {
    fn reference(&self, kind: char, present: bool, version: u64) -> Term {
        if present {
            Term::Bin(keys::value_ref(
                self.partition,
                kind,
                self.record.id().as_bytes(),
                version,
            ))
        } else {
            Term::Nil
        }
    }

    fn value_refs(&self) -> Term {
        let owner = self.record.id().as_bytes();
        Term::Map(
            self.meta
                .values
                .iter()
                .map(|(name, value)| {
                    let entry = map(vec![
                        ("version", Term::int(value.version)),
                        (
                            "ref",
                            Term::Bin(keys::named_value_ref(
                                self.partition,
                                owner,
                                name,
                                value.version,
                            )),
                        ),
                        ("digest", Term::Bin(keys::digest(&value.bytes))),
                    ]);
                    (name.clone(), entry)
                })
                .collect(),
        )
    }

    /// When a terminal Flow is removed.
    #[must_use]
    pub fn terminal_retention_until(&self) -> Option<u64> {
        let terminal = matches!(
            self.record.status(),
            WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled
        );
        if !terminal {
            return None;
        }
        if let Some(until) = self.meta.terminal_until {
            return Some(until);
        }
        Some({
            self.record
                .updated_at()
                .get()
                .saturating_add(self.meta.retention_ttl_ms)
        })
    }

    /// The reply map, with what `include` asks for.
    #[must_use]
    pub fn term(&self, include: &Include) -> Term {
        let record = self.record;
        let meta = self.meta;
        let mut entries: Vec<(&str, Term)> = Vec::with_capacity(40);
        if !meta.attributes.is_empty() {
            entries.push(("attributes", meta::attributes_term(&meta.attributes)));
        }
        entries.extend([
            ("id", Term::Bin(record.id().as_bytes().to_vec())),
            ("priority", Term::Int(i64::from(self.priority))),
            ("type", Term::Bin(record.workflow_type().to_vec())),
            ("version", Term::int(record.version())),
        ]);
        let mut values: Vec<(Vec<u8>, Term)> = match &include.values {
            Values::None => Vec::new(),
            Values::All => meta
                .values
                .iter()
                .map(|(name, value)| (name.clone(), Term::Bin(value.bytes.clone())))
                .collect(),
            Values::Names(names) => names
                .iter()
                .filter_map(|name| {
                    let value = meta.values.get(name)?;
                    Some((name.clone(), Term::Bin(value.bytes.clone())))
                })
                .collect(),
        };
        if !values.is_empty() {
            // Elixir's small maps list their keys in order.
            values.sort_by(|left, right| left.0.cmp(&right.0));
            values.dedup_by(|left, right| left.0 == right.0);
            entries.push(("values", Term::Map(values)));
        }
        self.lineage(&mut entries);
        self.lease(&mut entries);
        if !meta.state_meta.is_empty() {
            entries.push(("state_meta", meta::state_meta_term(&meta.state_meta)));
        }
        let lease = record.lease();
        entries.extend([
            (
                "lease_owner",
                Term::optional_bin(lease.map(|lease| lease.worker.as_slice())),
            ),
            (
                "terminal_retention_until_ms",
                Term::optional_int(self.terminal_retention_until()),
            ),
        ]);
        if !meta.values.is_empty() {
            entries.push(("value_refs", self.value_refs()));
        }
        entries.extend([
            (
                "child_groups",
                Term::Map(
                    meta.child_groups
                        .iter()
                        .map(|(id, group)| (id.clone(), group.term()))
                        .collect(),
                ),
            ),
            (
                "parent_partition_key",
                Term::optional_bin(meta.parent_partition_key.as_deref()),
            ),
            (
                "error_ref",
                self.reference('e', record.error().is_some(), meta.error_version),
            ),
            (
                "result_ref",
                self.reference('r', record.result().is_some(), meta.result_version),
            ),
        ]);
        if include.payload {
            for (kind, value) in [
                ("payload", record.payload()),
                ("result", record.result()),
                ("error", record.error()),
            ] {
                hydrate(&mut entries, kind, value, include.max_bytes);
            }
        }
        map(entries)
    }

    /// State, references, lineage, partition and retention settings.
    fn lineage(&self, entries: &mut Vec<(&str, Term)>) {
        let record = self.record;
        let meta = self.meta;
        let id = record.id().as_bytes();
        entries.extend([
            ("state", Term::Bin(record.state().to_vec())),
            (
                "payload_ref",
                self.reference('p', record.payload().is_some(), meta.payload_version),
            ),
            (
                "parent_flow_id",
                Term::optional_bin(meta.parent_flow_id.as_deref()),
            ),
            (
                "root_flow_id",
                Term::Bin(meta.root_flow_id.clone().unwrap_or_else(|| id.to_vec())),
            ),
            (
                "correlation_id",
                Term::optional_bin(meta.correlation_id.as_deref()),
            ),
            ("partition_key", Term::Bin(self.partition.name())),
            ("retention_ttl_ms", Term::int(meta.retention_ttl_ms)),
        ]);
        match meta.max_active_ms {
            Some(MaxActive::Ms(value)) => entries.push(("max_active_ms", Term::int(value))),
            Some(MaxActive::Infinity) => entries.push(("max_active_ms", Term::text("infinity"))),
            None => {}
        }
        entries.extend([
            (
                "history_hot_max_events",
                Term::int(meta.history_hot_max_events),
            ),
            ("history_max_events", Term::int(meta.history_max_events)),
            ("ttl_ms", Term::optional_int(meta.ttl_ms)),
        ]);
    }

    /// Fence, lease, times, attempts and run state.
    fn lease(&self, entries: &mut Vec<(&str, Term)>) {
        let record = self.record;
        let lease = record.lease();
        entries.extend([
            ("fencing_token", Term::int(record.fence().get())),
            (
                "lease_token",
                Term::optional_bin(lease.map(|lease| lease.token.as_bytes())),
            ),
            ("created_at_ms", Term::int(record.created_at().get())),
            ("updated_at_ms", Term::int(record.updated_at().get())),
            ("next_run_at_ms", ms(record.next_run_at())),
            (
                "lease_deadline_ms",
                Term::int(lease.map_or(0, |lease| lease.deadline.get())),
            ),
            ("attempts", Term::int(record.attempts())),
            ("run_state", Term::optional_bin(record.run_state())),
        ]);
    }
}
