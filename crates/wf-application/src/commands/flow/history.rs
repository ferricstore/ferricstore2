//! Flow history (ADR-014 step 4): one event per record version, and
//! `FLOW.HISTORY` read as `Ferricstore.Flow.HistoryAPI` reads it.
//!
//! An event keeps what changes between versions; the reply adds what never
//! changes (id, type, creation time, lineage). Every reply field is text, as
//! in Elixir's history streams, with an empty string for nil. Event ids are
//! `<at>-<version>`.

use wf_domain::history::{EventKind, RetryHistory};
use wf_domain::workflow::WorkflowRecord;

use super::FlowError;
use super::create::partition_key;
use super::keys;
use super::opts::Opts;
use super::record::{DEFAULT_HISTORY_MAX_EVENTS, FlowMeta};
use super::term::{Term, map};
use crate::routing::Partition;

/// Most events one read returns.
const MAX_COUNT: u64 = 10_000;

/// What a history event records: a lifecycle change the domain decided, or
/// a named value replaced (`value_put`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryKind {
    /// A lifecycle change.
    Event(EventKind),
    /// A named value written by `FLOW.VALUE.PUT`.
    ValuePut,
}

impl HistoryKind {
    /// Elixir's event name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Event(kind) => kind.as_str(),
            Self::ValuePut => "value_put",
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
}

impl HistoryEvent {
    /// The event for a record's latest version.
    #[must_use]
    pub fn of(record: &WorkflowRecord, priority: u8, meta: &FlowMeta) -> Self {
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

/// Which events a read returns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryQuery {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if sent.
    pub partition_key: Option<Vec<u8>>,
    /// Most events.
    pub count: usize,
    /// Event id bounds, inclusive.
    pub from_event: Option<(u64, u64)>,
    /// Upper event id bound.
    pub to_event: Option<(u64, u64)>,
    /// Time bounds, inclusive.
    pub from_ms: Option<u64>,
    /// Upper time bound.
    pub to_ms: Option<u64>,
    /// Version bounds, inclusive.
    pub from_version: Option<u64>,
    /// Upper version bound.
    pub to_version: Option<u64>,
    /// Newest first.
    pub rev: bool,
    /// Only this event kind.
    pub event: Option<Vec<u8>>,
    /// Only events whose lease owner is this worker.
    pub worker: Option<Vec<u8>>,
}

const ALLOWED: &[&str] = &[
    "id",
    "partition_key",
    "count",
    "from_event",
    "to_event",
    "from_ms",
    "to_ms",
    "from_version",
    "to_version",
    "rev",
    "event",
    "worker",
    "include_cold",
    "consistent_projection",
    "values",
    "max_bytes",
];

fn event_id(opts: &Opts<'_>, key: &str) -> Result<Option<(u64, u64)>, FlowError> {
    let Some(text) = opts.binary_or_nil(key)? else {
        return Ok(None);
    };
    let invalid = || FlowError::flow(key, "must be a history event id");
    let text = std::str::from_utf8(&text).map_err(|_| invalid())?;
    let (ms, version) = text.split_once('-').ok_or_else(invalid)?;
    let canonical = |part: &str| {
        (!part.is_empty()
            && (part == "0" || !part.starts_with('0'))
            && part.bytes().all(|b| b.is_ascii_digit()))
        .then(|| part.parse::<u64>().ok())
        .flatten()
    };
    Ok(Some((
        canonical(ms).ok_or_else(invalid)?,
        canonical(version).ok_or_else(invalid)?,
    )))
}

/// Reads `FLOW.HISTORY`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse(args: &Term) -> Result<HistoryQuery, FlowError> {
    let opts = Opts::new("FLOW.HISTORY", args, ALLOWED)?;
    let id = match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
        _ => return Err(FlowError::flow("id", "must be a non-empty string")),
    };
    let count = match opts.positive_integer("count")? {
        None => MAX_COUNT,
        Some(count) if count > MAX_COUNT => {
            return Err(FlowError::flow(
                "count",
                &format!("exceeds maximum {MAX_COUNT}"),
            ));
        }
        Some(count) => count,
    };
    let query = HistoryQuery {
        id,
        partition_key: partition_key(&opts)?,
        count: usize::try_from(count).unwrap_or(usize::MAX),
        from_event: event_id(&opts, "from_event")?,
        to_event: event_id(&opts, "to_event")?,
        from_ms: opts.non_neg_integer("from_ms")?,
        to_ms: opts.non_neg_integer("to_ms")?,
        from_version: opts.non_neg_integer("from_version")?,
        to_version: opts.non_neg_integer("to_version")?,
        rev: opts.boolean("rev", false)?,
        event: opts.binary_or_nil("event")?,
        worker: opts.binary_or_nil("worker")?,
    };
    let ordered =
        |low: Option<u64>, high: Option<u64>| low.zip(high).is_none_or(|(low, high)| low <= high);
    if !ordered(query.from_ms, query.to_ms) {
        return Err(FlowError::flow("from_ms", "must be <= to_ms"));
    }
    if !ordered(query.from_version, query.to_version) {
        return Err(FlowError::flow("from_version", "must be <= to_version"));
    }
    if query
        .from_event
        .zip(query.to_event)
        .is_some_and(|(from, to)| from > to)
    {
        return Err(FlowError::flow("from_event", "must be <= to_event"));
    }
    opts.boolean("include_cold", true)?;
    opts.boolean("consistent_projection", true)?;
    Ok(query)
}

impl HistoryQuery {
    fn matches(&self, event: &HistoryEvent) -> bool {
        let key = (event.at, event.version);
        self.from_event.is_none_or(|from| key >= from)
            && self.to_event.is_none_or(|to| key <= to)
            && self.from_ms.is_none_or(|from| event.at >= from)
            && self.to_ms.is_none_or(|to| event.at <= to)
            && self.from_version.is_none_or(|from| event.version >= from)
            && self.to_version.is_none_or(|to| event.version <= to)
            && self
                .event
                .as_ref()
                .is_none_or(|name| name.as_slice() == event.kind.as_str().as_bytes())
            && self.worker.as_ref().is_none_or(|worker| {
                event
                    .lease
                    .as_ref()
                    .is_some_and(|(owner, _)| owner == worker)
            })
    }

    /// The selected events: newest first up to the count when `rev`,
    /// otherwise the newest `count` in order.
    #[must_use]
    pub fn select<'a>(&self, history: &'a [HistoryEvent]) -> Vec<&'a HistoryEvent> {
        let matching: Vec<&HistoryEvent> =
            history.iter().filter(|event| self.matches(event)).collect();
        if self.rev {
            matching.into_iter().rev().take(self.count).collect()
        } else {
            let skip = matching.len().saturating_sub(self.count);
            matching.into_iter().skip(skip).collect()
        }
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
