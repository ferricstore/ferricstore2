//! `FLOW.HISTORY` options and event selection, as
//! `Ferricstore.Flow.HistoryAPI` and `HistoryQuery` read and apply them.

use super::super::FlowError;
use super::super::create::partition_key;
use super::super::opts::Opts;
use super::super::record::DEFAULT_PAYLOAD_MAX_BYTES;
use super::super::term::Term;
use super::HistoryEvent;

/// Most events one read returns.
const MAX_COUNT: u64 = 10_000;

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
    /// Each event also shows the payload, result and error it named
    /// (ADR-016 phase C).
    pub values: bool,
    /// The largest value shown; a larger one is omitted with its size
    /// (audit gap G36).
    pub max_bytes: u64,
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
    "payload_max_bytes",
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

/// `payload_max_bytes`, as Elixir's `PayloadReturn.history_options` reads
/// it: the default ceiling when absent, refused above it.
fn max_bytes(opts: &Opts<'_>) -> Result<u64, FlowError> {
    let max_bytes = opts
        .non_neg_integer("payload_max_bytes")?
        .unwrap_or(DEFAULT_PAYLOAD_MAX_BYTES);
    if max_bytes > DEFAULT_PAYLOAD_MAX_BYTES {
        return Err(FlowError::flow(
            "payload_max_bytes",
            &format!("exceeds maximum {DEFAULT_PAYLOAD_MAX_BYTES}"),
        ));
    }
    Ok(max_bytes)
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
        values: opts.boolean("values", false)?,
        max_bytes: max_bytes(&opts)?,
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
