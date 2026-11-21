//! A stored Flow as Elixir's `RecordProjection.public/1` shows it.

use wf_domain::workflow::{WorkflowRecord, WorkflowStatus};
use wf_types::version::TimestampMillis;

use super::super::create::MaxActive;
use super::super::keys;
use super::super::meta;
use super::super::term::{Term, map};
use super::super::value_codec;
use super::{FlowMeta, Include, Values};
use crate::routing::Partition;

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
        entries.push((field, value_codec::load(value)));
    }
    entries.push((size_field, Term::int(size)));
}

/// A read's payload fields for `value`, read from the stored value a Flow
/// names as its payload (audit gap G16).
#[must_use]
pub fn payload_fields(value: &[u8], max_bytes: u64) -> Vec<(&'static str, Term)> {
    let mut entries = Vec::new();
    hydrate(&mut entries, "payload", Some(value), max_bytes);
    entries
}

/// An attached reference as `value_refs` shows it: no digest or version
/// (audit gap G9).
#[must_use]
pub fn attached(reference: &[u8]) -> Term {
    map(vec![
        ("digest", Term::Nil),
        ("ref", Term::Bin(reference.to_vec())),
        ("version", Term::Nil),
    ])
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
                .chain(
                    self.meta
                        .value_refs
                        .iter()
                        .map(|(name, reference)| (name.clone(), attached(reference))),
                )
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
                .map(|(name, value)| (name.clone(), value_codec::load(&value.bytes)))
                .collect(),
            Values::Names(names) => names
                .iter()
                .filter_map(|name| {
                    let value = meta.values.get(name)?;
                    Some((name.clone(), value_codec::load(&value.bytes)))
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
        if !meta.values.is_empty() || !meta.value_refs.is_empty() {
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
                match &meta.payload_ref {
                    Some(reference) => Term::Bin(reference.clone()),
                    None => self.reference('p', record.payload().is_some(), meta.payload_version),
                },
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
        if let Some(event) = &meta.rewound_to {
            entries.push(("rewound_to_event_id", Term::Bin(event.clone())));
        }
        if !meta.indexed_attributes.is_empty() {
            let names = meta.indexed_attributes.iter().map(|name| Term::text(name));
            entries.push(("indexed_attributes", Term::List(names.collect())));
        }
        if let Some(key) = &meta.indexed_state_meta {
            entries.push(("indexed_state_meta", Term::text(key)));
        }
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
