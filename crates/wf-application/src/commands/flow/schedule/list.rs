//! `FLOW.SCHEDULE.LIST`, as `Schedule.Listing` filters and orders
//! schedules, and the `FIRE_DUE` summary.

use super::super::FlowError;
use super::super::opts::Opts;
use super::super::term::{Term, map};
use super::definition::Kind;
use super::integer;

const STATES: [&str; 6] = [
    "active",
    "paused",
    "running",
    "completed",
    "failed",
    "cancelled",
];
const MAX_COUNT: u64 = 1_000;

/// A listing request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// The states listed.
    pub states: Vec<Vec<u8>>,
    /// Only this kind.
    pub kind: Option<Kind>,
    /// Only targets of this type.
    pub target_type: Option<Vec<u8>>,
    /// Only this zone.
    pub timezone: Option<Vec<u8>>,
    /// Due at or after.
    pub from_ms: Option<u64>,
    /// Due at or before.
    pub to_ms: Option<u64>,
    /// Most schedules listed.
    pub count: usize,
    /// Latest due first.
    pub rev: bool,
}

fn rejected(text: impl Into<String>) -> FlowError {
    FlowError::rejected(text.into())
}

fn text(opts: &Opts<'_>, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get(key) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(rejected(format!(
            "ERR flow schedule {key} must be a non-empty string"
        ))),
    }
}

/// Reads a listing request.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn parse(args: &Term) -> Result<List, FlowError> {
    let opts = Opts::new(
        "FLOW.SCHEDULE.LIST",
        args,
        &[
            "state",
            "kind",
            "target_type",
            "timezone",
            "from_ms",
            "to_ms",
            "count",
            "rev",
            "deadline_ms",
        ],
    )?;
    let states = match opts.get("state") {
        None => vec![b"active".to_vec()],
        Some(Term::Bin(state)) if state.as_slice() == b"all" => STATES
            .iter()
            .map(|state| state.as_bytes().to_vec())
            .collect(),
        Some(Term::Bin(state))
            if STATES
                .iter()
                .any(|known| known.as_bytes() == state.as_slice()) =>
        {
            vec![state.clone()]
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow schedule state must be active, paused, running, completed, failed, cancelled, or all",
            ));
        }
    };
    let count = match opts.get("count") {
        None => 100,
        Some(Term::Int(count)) if *count > 0 => {
            u64::try_from(*count).unwrap_or(MAX_COUNT).min(MAX_COUNT)
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow schedule count must be a positive integer",
            ));
        }
    };
    let rev = match opts.get("rev") {
        None => false,
        Some(Term::Bool(rev)) => *rev,
        Some(_) => return Err(rejected("ERR flow schedule rev must be a boolean")),
    };
    let kind = match opts.get("kind") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(name)) => Some(Kind::parse(name).ok_or_else(|| {
            rejected("ERR flow schedule kind must be :one_shot, :delay, :interval, or :cron")
        })?),
        Some(_) => {
            return Err(rejected(
                "ERR flow schedule kind must be :one_shot, :delay, :interval, or :cron",
            ));
        }
    };
    let target_type = text(&opts, "target_type")?;
    let timezone = text(&opts, "timezone")?.map(|zone| {
        if zone.as_slice() == b"UTC" {
            b"Etc/UTC".to_vec()
        } else {
            zone
        }
    });
    let from_ms = integer(&opts, "from_ms")?;
    let to_ms = integer(&opts, "to_ms")?;
    if let (Some(from), Some(to)) = (from_ms, to_ms)
        && from > to
    {
        return Err(rejected(
            "ERR flow schedule from_ms must be less than or equal to to_ms",
        ));
    }
    Ok(List {
        states,
        kind,
        target_type,
        timezone,
        from_ms,
        to_ms,
        count: usize::try_from(count).unwrap_or(usize::MAX),
        rev,
    })
}

fn due(view: &Term) -> Option<u64> {
    match view.get(b"next_run_at_ms") {
        Some(Term::Int(due)) => u64::try_from(*due).ok(),
        _ => None,
    }
}

fn same(expected: Option<&Vec<u8>>, actual: Option<&Term>) -> bool {
    expected.is_none_or(|expected| matches!(actual, Some(Term::Bin(actual)) if actual == expected))
}

impl List {
    /// Whether a schedule's view is listed.
    #[must_use]
    pub fn matches(&self, view: &Term) -> bool {
        let state = view.get(b"state");
        let in_range = match (due(view), self.from_ms, self.to_ms) {
            (None, None, None) => true,
            (None, ..) => false,
            (Some(due), from, to) => {
                from.is_none_or(|from| due >= from) && to.is_none_or(|to| due <= to)
            }
        };
        self.states
            .iter()
            .any(|listed| state == Some(&Term::Bin(listed.clone())))
            && self
                .kind
                .is_none_or(|kind| view.get(b"kind") == Some(&Term::text(kind.name())))
            && same(
                self.target_type.as_ref(),
                view.get(b"target").and_then(|target| target.get(b"type")),
            )
            && same(self.timezone.as_ref(), view.get(b"timezone"))
            && in_range
    }

    /// The listed views, ordered (due ones first by time, then id) and cut
    /// to `count`.
    #[must_use]
    pub fn select(&self, views: Vec<Term>) -> Term {
        let mut kept: Vec<Term> = views
            .into_iter()
            .filter(|view| self.matches(view))
            .collect();
        let key = |view: &Term| {
            let id = match view.get(b"id") {
                Some(Term::Bin(id)) => id.clone(),
                _ => Vec::new(),
            };
            (due(view).is_none(), due(view).unwrap_or(0), id)
        };
        kept.sort_by(|left, right| {
            let (left, right) = (key(left), key(right));
            if self.rev {
                // Due ones first, latest first; undue ones after, by id down.
                left.0
                    .cmp(&right.0)
                    .then(right.1.cmp(&left.1))
                    .then(right.2.cmp(&left.2))
            } else {
                left.cmp(&right)
            }
        });
        kept.truncate(self.count);
        Term::List(kept)
    }
}

/// One claimed schedule's fire outcome, for the `FIRE_DUE` summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fired {
    /// A target was created.
    Fired {
        /// Its id.
        target_id: Vec<u8>,
        /// Missed periods coalesced.
        coalesced: u64,
    },
    /// The occurrence was held back.
    Skipped {
        /// Why.
        reason: String,
        /// Missed periods coalesced.
        coalesced: u64,
    },
    /// The fire failed.
    Failed {
        /// The schedule id.
        id: Vec<u8>,
        /// Why.
        reason: String,
    },
}

/// `Summary.fire_results/2`.
#[must_use]
pub fn summary(outcomes: &[Fired]) -> Term {
    let (mut fired, mut skipped, mut coalesced) = (0_u64, 0_u64, 0_u64);
    let mut last_target = None;
    let mut last_skip = None;
    let mut errors = Vec::new();
    for outcome in outcomes {
        match outcome {
            Fired::Fired {
                target_id,
                coalesced: count,
            } => {
                fired = fired.saturating_add(1);
                coalesced = coalesced.saturating_add(*count);
                last_target = Some(target_id.clone());
            }
            Fired::Skipped {
                reason,
                coalesced: count,
            } => {
                skipped = skipped.saturating_add(1);
                coalesced = coalesced.saturating_add(*count);
                last_skip = Some(reason.clone());
            }
            Fired::Failed { id, reason } => {
                errors.push(Term::List(vec![Term::Bin(id.clone()), Term::text(reason)]));
            }
        }
    }
    let mut entries = vec![
        (
            "claimed",
            Term::int(u64::try_from(outcomes.len()).unwrap_or(u64::MAX)),
        ),
        ("coalesced", Term::int(coalesced)),
        ("errors", Term::List(errors)),
        ("fired", Term::int(fired)),
        ("skipped", Term::int(skipped)),
    ];
    entries.extend(last_target.map(|id| ("last_target_id", Term::Bin(id))));
    entries.extend(last_skip.map(|reason| ("last_skip_reason", Term::text(&reason))));
    entries.sort_by_key(|(key, _)| *key);
    map(entries)
}
