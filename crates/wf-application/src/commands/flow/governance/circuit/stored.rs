//! A circuit's stored form: its reply map, encoded.

use super::super::super::term::Term;
use super::{Circuit, Event};

fn text(term: &Term, key: &str) -> Option<Vec<u8>> {
    match term.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Some(value.clone()),
        _ => None,
    }
}

fn int(term: &Term, key: &str) -> Option<u64> {
    match term.get(key.as_bytes()) {
        Some(Term::Int(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

/// A stored field that is neither nil nor of its type.
struct Damaged;

fn optional_int(term: &Term, key: &str) -> Result<Option<u64>, Damaged> {
    match term.get(key.as_bytes()) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Int(value)) => u64::try_from(*value).map(Some).map_err(|_| Damaged),
        Some(_) => Err(Damaged),
    }
}

fn texts(term: &Term, key: &str) -> Option<Vec<Vec<u8>>> {
    match term.get(key.as_bytes()) {
        Some(Term::List(items)) => items
            .iter()
            .map(|item| match item {
                Term::Bin(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn event(term: &Term) -> Option<Event> {
    let error_class = match term.get(b"error_class") {
        None => None,
        Some(Term::Bin(class)) => Some(class.clone()),
        Some(_) => return None,
    };
    Some(Event {
        at_ms: int(term, "at_ms")?,
        kind: text(term, "kind")?,
        status: text(term, "status")?,
        failures: int(term, "failures")?,
        counted: matches!(term.get(b"counted"), Some(Term::Bool(true))),
        latency_ms: optional_int(term, "latency_ms").ok()?,
        error_class,
    })
}

impl Circuit {
    /// The stored form.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        self.view().encode(&mut out)?;
        Some(out)
    }

    /// Reads the stored form; `None` when it is not a circuit.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let Some((term, [])) = Term::decode(bytes) else {
            return None;
        };
        let events = match term.get(b"events") {
            Some(Term::List(items)) => items.iter().map(event).collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        let status = text(&term, "status")?;
        if !matches!(status.as_slice(), b"closed" | b"open" | b"half_open") {
            return None;
        }
        Some(Self {
            scope: text(&term, "scope")?,
            failure_threshold: int(&term, "failure_threshold")?,
            open_ms: int(&term, "open_ms")?,
            opened_at_ms: optional_int(&term, "opened_at_ms").ok()?,
            window_ms: optional_int(&term, "window_ms").ok()?,
            min_calls: optional_int(&term, "min_calls").ok()?,
            failure_rate_pct: optional_int(&term, "failure_rate_pct").ok()?,
            latency_threshold_ms: optional_int(&term, "latency_threshold_ms").ok()?,
            error_classes: texts(&term, "error_classes")?,
            half_open_max_probes: int(&term, "half_open_max_probes")?,
            half_open_success_threshold: int(&term, "half_open_success_threshold")?,
            half_open_in_flight: int(&term, "half_open_in_flight")?,
            half_open_successes: int(&term, "half_open_successes")?,
            half_open_started_at_ms: optional_int(&term, "half_open_started_at_ms").ok()?,
            last_failure_ms: optional_int(&term, "last_failure_ms").ok()?,
            last_success_ms: optional_int(&term, "last_success_ms").ok()?,
            updated_at_ms: optional_int(&term, "updated_at_ms").ok()?,
            events,
            status,
            failures: int(&term, "failures")?,
        })
    }
}
