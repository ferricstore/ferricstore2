//! Circuits, as `Governance.Circuit` and `CircuitStore` keep them: a
//! scope's breaker with its rule and recent events. The typed protocol
//! opens and closes one by hand and reads it; no reachable policy records
//! effect outcomes against one (ADR-014 step 14).
//!
//! A manual open (re)configures the rule from the options given and opens
//! the circuit; a manual close resets its failures. Each appends two events
//! (the state change, then the manual action), newest first, keeping the
//! newest 64 and, with a `window_ms`, those within the window.

use super::super::FlowError;
use super::super::term::{Term, map};
use super::{Change, Decided};

mod request;
mod stored;

pub use request::parse;

const MAX_EVENTS: usize = 64;

/// The rule options a manual open sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    /// Failures that open the circuit.
    pub failure_threshold: Option<u64>,
    /// How long it stays open.
    pub open_ms: Option<u64>,
    /// The tracking window.
    pub window_ms: Option<u64>,
    /// Calls needed before a failure rate counts.
    pub min_calls: Option<u64>,
    /// The failure rate that opens it.
    pub failure_rate_pct: Option<u64>,
    /// Latency that counts as a failure.
    pub latency_threshold_ms: Option<u64>,
    /// Error classes that count.
    pub error_classes: Option<Vec<Vec<u8>>>,
    /// Probes allowed half-open.
    pub half_open_max_probes: Option<u64>,
    /// Probe successes that close it.
    pub half_open_success_threshold: Option<u64>,
}

/// One event in a circuit's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// When.
    pub at_ms: u64,
    /// What happened.
    pub kind: Vec<u8>,
    /// The status after it.
    pub status: Vec<u8>,
    /// The failures after it.
    pub failures: u64,
    /// Whether it counted toward the failure rate.
    pub counted: bool,
    /// The call's latency, when recorded.
    pub latency_ms: Option<u64>,
    /// The call's error class, when recorded.
    pub error_class: Option<Vec<u8>>,
}

/// One circuit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Circuit {
    /// Its scope.
    pub scope: Vec<u8>,
    /// Its rule; unset optional parts are nil.
    pub failure_threshold: u64,
    /// How long it stays open.
    pub open_ms: u64,
    /// When it opened.
    pub opened_at_ms: Option<u64>,
    /// The tracking window.
    pub window_ms: Option<u64>,
    /// Calls needed before a failure rate counts.
    pub min_calls: Option<u64>,
    /// The failure rate that opens it.
    pub failure_rate_pct: Option<u64>,
    /// Latency that counts as a failure.
    pub latency_threshold_ms: Option<u64>,
    /// Error classes that count.
    pub error_classes: Vec<Vec<u8>>,
    /// Probes allowed half-open.
    pub half_open_max_probes: u64,
    /// Probe successes that close it.
    pub half_open_success_threshold: u64,
    /// Probes in flight.
    pub half_open_in_flight: u64,
    /// Probe successes so far.
    pub half_open_successes: u64,
    /// When it went half-open.
    pub half_open_started_at_ms: Option<u64>,
    /// The last failure.
    pub last_failure_ms: Option<u64>,
    /// The last success.
    pub last_success_ms: Option<u64>,
    /// The last change.
    pub updated_at_ms: Option<u64>,
    /// Recent events, newest first.
    pub events: Vec<Event>,
    /// `closed`, `open` or `half_open`.
    pub status: Vec<u8>,
    /// Failures counted.
    pub failures: u64,
}

/// A command on one circuit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `FLOW.CIRCUIT.OPEN`.
    Open {
        /// The scope.
        scope: Vec<u8>,
        /// The client's clock.
        now_ms: Option<u64>,
        /// The rule options given.
        rule: Box<Rule>,
    },
    /// `FLOW.CIRCUIT.CLOSE`.
    Close {
        /// The scope.
        scope: Vec<u8>,
        /// The client's clock.
        now_ms: Option<u64>,
    },
    /// `FLOW.CIRCUIT.GET`.
    Get(Vec<u8>),
}

impl Circuit {
    /// `Circuit.new/2` with the defaults a manual open or close starts from.
    fn new(scope: &[u8], failure_threshold: u64) -> Self {
        Self {
            scope: scope.to_vec(),
            failure_threshold,
            open_ms: 30_000,
            opened_at_ms: None,
            window_ms: None,
            min_calls: None,
            failure_rate_pct: None,
            latency_threshold_ms: None,
            error_classes: Vec::new(),
            half_open_max_probes: 1,
            half_open_success_threshold: 1,
            half_open_in_flight: 0,
            half_open_successes: 0,
            half_open_started_at_ms: None,
            last_failure_ms: None,
            last_success_ms: None,
            updated_at_ms: None,
            events: Vec::new(),
            status: b"closed".to_vec(),
            failures: 0,
        }
    }

    /// `Circuit.configure/2`: sets each rule option given.
    fn configure(&mut self, rule: &Rule) {
        let set = |slot: &mut u64, value: Option<u64>| {
            if let Some(value) = value {
                *slot = value;
            }
        };
        set(&mut self.failure_threshold, rule.failure_threshold);
        set(&mut self.open_ms, rule.open_ms);
        for (slot, value) in [
            (&mut self.window_ms, rule.window_ms),
            (&mut self.min_calls, rule.min_calls),
            (&mut self.failure_rate_pct, rule.failure_rate_pct),
            (&mut self.latency_threshold_ms, rule.latency_threshold_ms),
        ] {
            if value.is_some() {
                *slot = value;
            }
        }
        if let Some(classes) = &rule.error_classes {
            self.error_classes.clone_from(classes);
        }
        set(&mut self.half_open_max_probes, rule.half_open_max_probes);
        set(
            &mut self.half_open_success_threshold,
            rule.half_open_success_threshold,
        );
    }

    /// Appends an uncounted event, newest first, within the bounds.
    fn record(&mut self, now: u64, kind: &str) {
        self.events.insert(
            0,
            Event {
                at_ms: now,
                kind: kind.as_bytes().to_vec(),
                status: self.status.clone(),
                failures: self.failures,
                counted: false,
                latency_ms: None,
                error_class: None,
            },
        );
        self.events.truncate(MAX_EVENTS);
        if let Some(window) = self.window_ms {
            self.events
                .retain(|event| now.saturating_sub(event.at_ms) <= window);
        }
    }

    fn reset_probes(&mut self, now: u64) {
        self.half_open_in_flight = 0;
        self.half_open_successes = 0;
        self.half_open_started_at_ms = None;
        self.updated_at_ms = Some(now);
    }

    fn open(&mut self, now: u64) {
        self.status = b"open".to_vec();
        self.opened_at_ms = Some(now);
        self.reset_probes(now);
        self.record(now, "opened");
        self.record(now, "manual_open");
    }

    fn close(&mut self, now: u64) {
        self.status = b"closed".to_vec();
        self.failures = 0;
        self.opened_at_ms = None;
        self.last_success_ms = Some(now);
        self.reset_probes(now);
        self.record(now, "closed");
        self.record(now, "manual_close");
    }

    /// The circuit as Elixir replies with it.
    #[must_use]
    pub fn view(&self) -> Term {
        let events = self
            .events
            .iter()
            .map(|event| {
                let mut entries = vec![
                    ("at_ms", Term::int(event.at_ms)),
                    ("kind", Term::Bin(event.kind.clone())),
                    ("status", Term::Bin(event.status.clone())),
                    ("failures", Term::int(event.failures)),
                    ("counted", Term::Bool(event.counted)),
                ];
                entries.extend(
                    event
                        .latency_ms
                        .map(|latency| ("latency_ms", Term::int(latency))),
                );
                entries.extend(
                    event
                        .error_class
                        .clone()
                        .map(|class| ("error_class", Term::Bin(class))),
                );
                map(entries)
            })
            .collect();
        map(vec![
            ("scope", Term::Bin(self.scope.clone())),
            ("failure_threshold", Term::int(self.failure_threshold)),
            ("open_ms", Term::int(self.open_ms)),
            ("opened_at_ms", Term::optional_int(self.opened_at_ms)),
            ("window_ms", Term::optional_int(self.window_ms)),
            ("min_calls", Term::optional_int(self.min_calls)),
            (
                "failure_rate_pct",
                Term::optional_int(self.failure_rate_pct),
            ),
            (
                "latency_threshold_ms",
                Term::optional_int(self.latency_threshold_ms),
            ),
            (
                "error_classes",
                Term::List(self.error_classes.iter().cloned().map(Term::Bin).collect()),
            ),
            ("half_open_max_probes", Term::int(self.half_open_max_probes)),
            (
                "half_open_success_threshold",
                Term::int(self.half_open_success_threshold),
            ),
            ("half_open_in_flight", Term::int(self.half_open_in_flight)),
            ("half_open_successes", Term::int(self.half_open_successes)),
            (
                "half_open_started_at_ms",
                Term::optional_int(self.half_open_started_at_ms),
            ),
            ("last_failure_ms", Term::optional_int(self.last_failure_ms)),
            ("last_success_ms", Term::optional_int(self.last_success_ms)),
            ("updated_at_ms", Term::optional_int(self.updated_at_ms)),
            ("events", Term::List(events)),
            ("status", Term::Bin(self.status.clone())),
            ("failures", Term::int(self.failures)),
            ("failure_count", Term::int(self.failures)),
            (
                "event_count",
                Term::int(u64::try_from(self.events.len()).unwrap_or(u64::MAX)),
            ),
        ])
    }
}

impl Command {
    /// The circuit's scope.
    #[must_use]
    pub fn scope(&self) -> &[u8] {
        match self {
            Self::Open { scope, .. } | Self::Close { scope, .. } | Self::Get(scope) => scope,
        }
    }

    /// Decides the command from the circuit's stored form.
    pub fn decide(&self, stored: Option<&[u8]>, wall: u64) -> Decided {
        let existing = match stored.map(Circuit::decode) {
            None => None,
            Some(Some(circuit)) => Some(circuit),
            Some(None) => {
                return (
                    Err(FlowError::rejected(
                        "ERR flow circuit record is corrupt".to_owned(),
                    )),
                    None,
                );
            }
        };
        let (now, rule, mut circuit) = match self {
            Self::Get(_) => {
                return (
                    Ok(existing.map_or(Term::Nil, |circuit| circuit.view())),
                    None,
                );
            }
            Self::Open {
                scope,
                now_ms,
                rule,
            } => (
                now_ms.unwrap_or(wall),
                Some(rule.as_ref()),
                existing.unwrap_or_else(|| Circuit::new(scope, 5)),
            ),
            Self::Close { scope, now_ms } => (
                now_ms.unwrap_or(wall),
                None,
                existing.unwrap_or_else(|| Circuit::new(scope, 1)),
            ),
        };
        if circuit.updated_at_ms.is_some_and(|at| now < at) {
            return (
                Err(FlowError::rejected(
                    "ERR flow circuit now_ms cannot precede updated_at_ms".to_owned(),
                )),
                None,
            );
        }
        match rule {
            Some(rule) => {
                circuit.configure(rule);
                circuit.open(now);
            }
            None => circuit.close(now),
        }
        match circuit.encode() {
            Some(bytes) => (Ok(circuit.view()), Some(Change::Put(bytes))),
            None => (Err(FlowError::Unavailable), None),
        }
    }
}
