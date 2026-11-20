//! Flow history (ADR-014 step 4): one event per record version, and
//! `FLOW.HISTORY` read as `Ferricstore.Flow.HistoryAPI` reads it.
//!
//! An event keeps what changes between versions; the reply adds what never
//! changes (id, type, creation time, lineage). Every reply field is text, as
//! in Elixir's history streams, with an empty string for nil. Event ids are
//! `<at>-<version>`.

mod query;

pub use query::{HistoryQuery, parse};

use wf_domain::history::{EventKind, RetryHistory};
use wf_domain::workflow::WorkflowRecord;

use super::record::{DEFAULT_HISTORY_MAX_EVENTS, FlowMeta};
use super::term::{Term, map};
use super::{json, keys, meta};
use crate::routing::Partition;

/// What a history event records: a lifecycle change the domain decided, or
/// a named value replaced (`value_put`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryKind {
    /// A lifecycle change.
    Event(EventKind),
    /// A named value written by `FLOW.VALUE.PUT`.
    ValuePut,
    /// A `FLOW.SIGNAL`.
    Signaled,
}

impl HistoryKind {
    /// Elixir's event name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Event(kind) => kind.as_str(),
            Self::ValuePut => "value_put",
            Self::Signaled => "signaled",
        }
    }
}

/// One history event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEvent {
    /// What happened.
    pub kind: HistoryKind,
    /// When: the record's update time.
    pub at: u64,
    /// Record version after the event.
    pub version: u64,
    /// State after the event.
    pub state: Vec<u8>,
    /// Fence after the event.
    pub fence: u64,
    /// Attempts after the event.
    pub attempts: u64,
    /// Lease owner and deadline after the event.
    pub lease: Option<(Vec<u8>, u64)>,
    /// Next run time after the event.
    pub next_run_at: Option<u64>,
    /// Claim priority after the event.
    pub priority: u8,
    /// Versions of the payload, result and error references, when present.
    pub refs: [Option<u64>; 3],
    /// The retry audit of a retry event.
    pub retry: Option<RetryHistory>,
    /// JSON of the Flow's value references, attributes and state metadata
    /// after the event, each when not empty, then a signal's name, then the
    /// reason and limit of an active-time timeout.
    pub extras: [Option<Vec<u8>>; 6],
}

/// Names of [`HistoryEvent::extras`], in order.
const EXTRAS: [&str; 6] = [
    "value_refs",
    "attributes",
    "state_meta",
    "signal",
    "reason",
    "max_active_ms",
];

/// The JSON extras of a Flow's fields, as Elixir adds them to each event.
fn extras(meta: &FlowMeta, partition: &Partition, id: &[u8]) -> [Option<Vec<u8>>; 6] {
    let value_refs = (!meta.values.is_empty()).then(|| {
        let refs = meta
            .values
            .iter()
            .map(|(name, value)| {
                let entry = map(vec![
                    ("digest", Term::Bin(keys::digest(&value.bytes))),
                    (
                        "ref",
                        Term::Bin(keys::named_value_ref(partition, id, name, value.version)),
                    ),
                    ("version", Term::int(value.version)),
                ]);
                (name.clone(), entry)
            })
            .collect();
        json::encode(&Term::Map(refs))
    });
    let attributes = (!meta.attributes.is_empty())
        .then(|| json::encode(&meta::attributes_term(&meta.attributes)));
    let state_meta = (!meta.state_meta.is_empty())
        .then(|| json::encode(&meta::state_meta_term(&meta.state_meta)));
    [value_refs, attributes, state_meta, None, None, None]
}

impl HistoryEvent {
    /// The event for a record's latest version.
    #[must_use]
    pub fn of(
        record: &WorkflowRecord,
        priority: u8,
        meta: &FlowMeta,
        partition: &Partition,
    ) -> Self {
        let (kind, _, _) = record.latest_event_witness();
        Self {
            kind: HistoryKind::Event(kind),
            at: record.updated_at().get(),
            version: record.version(),
            state: record.state().to_vec(),
            fence: record.fence().get(),
            attempts: record.attempts(),
            lease: record
                .lease()
                .map(|lease| (lease.worker.clone(), lease.deadline.get())),
            next_run_at: record
                .next_run_at()
                .map(wf_types::version::TimestampMillis::get),
            priority,
            refs: [
                record.payload().map(|_| meta.payload_version),
                record.result().map(|_| meta.result_version),
                record.error().map(|_| meta.error_version),
            ],
            retry: None,
            extras: extras(meta, partition, record.id().as_bytes()),
        }
    }

    /// The event id, `<at>-<version>`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}-{}", self.at, self.version)
    }
}

/// Appends `event`, keeping at most the Flow's history cap.
pub fn append(history: &mut Vec<HistoryEvent>, event: HistoryEvent, meta: &FlowMeta) {
    history.push(event);
    let cap = usize::try_from(meta.history_max_events.max(1)).unwrap_or(usize::MAX);
    if history.len() > cap {
        let excess = history.len().saturating_sub(cap);
        history.drain(..excess);
    }
}

fn text(value: impl std::fmt::Display) -> Term {
    Term::Bin(value.to_string().into_bytes())
}

fn bin_or_empty(value: Option<&[u8]>) -> Term {
    Term::Bin(value.map(<[u8]>::to_vec).unwrap_or_default())
}

/// The reply entry of one event: `[id, fields]`.
#[must_use]
pub fn event_term(
    event: &HistoryEvent,
    record: &WorkflowRecord,
    partition: &Partition,
    meta: &FlowMeta,
) -> Term {
    let id = record.id().as_bytes();
    let reference = |kind: char, version: Option<u64>| {
        Term::Bin(
            version
                .map(|version| keys::value_ref(partition, kind, id, version))
                .unwrap_or_default(),
        )
    };
    let [payload, result, error] = event.refs;
    let fields = map(vec![
        ("at", text(event.at)),
        ("attempts", text(event.attempts)),
        (
            "correlation_id",
            bin_or_empty(meta.correlation_id.as_deref()),
        ),
        ("created_at_ms", text(record.created_at().get())),
        ("error_ref", reference('e', error)),
        ("event", text(event.kind.as_str())),
        ("fencing_token", text(event.fence)),
        ("id", Term::Bin(id.to_vec())),
        (
            "lease_deadline_ms",
            event
                .lease
                .as_ref()
                .map_or_else(|| text(""), |(_, deadline)| text(deadline)),
        ),
        (
            "lease_owner",
            bin_or_empty(event.lease.as_ref().map(|(owner, _)| owner.as_slice())),
        ),
        (
            "next_run_at_ms",
            event.next_run_at.map_or_else(|| text(""), text),
        ),
        (
            "parent_flow_id",
            bin_or_empty(meta.parent_flow_id.as_deref()),
        ),
        ("payload_ref", reference('p', payload)),
        ("priority", text(event.priority)),
        ("result_ref", reference('r', result)),
        ("rewound_to_event_id", text("")),
        (
            "root_flow_id",
            Term::Bin(meta.root_flow_id.clone().unwrap_or_else(|| id.to_vec())),
        ),
        ("state", Term::Bin(event.state.clone())),
        ("type", Term::Bin(record.workflow_type().to_vec())),
        ("updated_at_ms", text(event.at)),
        ("version", text(event.version)),
    ]);
    let fields = match fields {
        Term::Map(mut entries) => {
            for (name, json) in EXTRAS.iter().zip(&event.extras) {
                if let Some(json) = json {
                    entries.push((name.as_bytes().to_vec(), Term::Bin(json.clone())));
                }
            }
            Term::Map(entries)
        }
        other => other,
    };
    let fields = match (&event.retry, fields) {
        (Some(retry), Term::Map(mut entries)) => {
            let policy = &retry.policy;
            entries.extend(
                [
                    ("retry_backoff_base_ms", text(policy.backoff.base_ms)),
                    ("retry_backoff_kind", text(policy.backoff.kind.as_str())),
                    ("retry_backoff_max_ms", text(policy.backoff.max_ms)),
                    ("retry_decision", text(retry.decision.as_str())),
                    ("retry_exhausted_to", Term::Bin(policy.exhausted_to.clone())),
                    ("retry_jitter_pct", text(policy.backoff.jitter_pct)),
                    ("retry_max_retries", text(policy.max_retries)),
                    (
                        "retry_next_run_at_ms",
                        retry
                            .next_run_at
                            .map_or_else(|| text(""), |at| text(at.get())),
                    ),
                    ("retry_run_state", Term::Bin(retry.run_state.clone())),
                ]
                .map(|(key, value)| (key.as_bytes().to_vec(), value)),
            );
            Term::Map(entries)
        }
        (_, fields) => fields,
    };
    Term::List(vec![text(event.id()), fields])
}

/// The default history cap of a Flow without its own.
#[must_use]
pub const fn default_cap() -> u64 {
    DEFAULT_HISTORY_MAX_EVENTS
}
