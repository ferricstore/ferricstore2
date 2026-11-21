//! A schedule's definition, the payload of its internal Flow, with the
//! fields of Elixir's definition map; stored as an encoded [`Term`] map and
//! shown by [`Definition::view`].

use super::super::term::{Term, map};

/// When a schedule fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Once, at a time.
    OneShot,
    /// Once, after a delay.
    Delay,
    /// Every `every_ms`.
    Interval,
    /// On a cron expression.
    Cron,
}

impl Kind {
    /// Elixir's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OneShot => "one_shot",
            Self::Delay => "delay",
            Self::Interval => "interval",
            Self::Cron => "cron",
        }
    }

    /// The kind named `name`.
    #[must_use]
    pub fn parse(name: &[u8]) -> Option<Self> {
        [Self::OneShot, Self::Delay, Self::Interval, Self::Cron]
            .into_iter()
            .find(|kind| kind.name().as_bytes() == name)
    }

    /// Interval and cron schedules fire more than once.
    #[must_use]
    pub const fn recurring(self) -> bool {
        matches!(self, Self::Interval | Self::Cron)
    }
}

/// What a recurring schedule does while its previous target still runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    /// Fire anyway.
    Allow,
    /// Skip this occurrence.
    Skip,
    /// Retry this occurrence later.
    QueueAfterPrevious,
    /// Fail the schedule.
    FailSchedule,
}

impl Overlap {
    /// Elixir's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Skip => "skip",
            Self::QueueAfterPrevious => "queue_after_previous",
            Self::FailSchedule => "fail_schedule",
        }
    }

    /// The policy named `name`.
    #[must_use]
    pub fn parse(name: &[u8]) -> Option<Self> {
        [
            Self::Allow,
            Self::Skip,
            Self::QueueAfterPrevious,
            Self::FailSchedule,
        ]
        .into_iter()
        .find(|policy| policy.name().as_bytes() == name)
    }
}

/// A schedule definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    /// The schedule id.
    pub id: Vec<u8>,
    /// Its kind.
    pub kind: Kind,
    /// The normalized target map.
    pub target: Term,
    /// The secret its targets' ownership digests are keyed with.
    pub secret: Vec<u8>,
    /// Creation time.
    pub created_at_ms: u64,
    /// The first due time.
    pub initial_run_at_ms: u64,
    /// The next due time; none once it ends.
    pub next_run_at_ms: Option<u64>,
    /// Fires so far.
    pub fire_count: u64,
    /// Interval period.
    pub every_ms: Option<u64>,
    /// Delay of a delay schedule.
    pub delay_ms: Option<u64>,
    /// Requested start of an interval or cron schedule.
    pub start_at_ms: Option<u64>,
    /// Cron expression.
    pub cron: Option<Vec<u8>>,
    /// Cron zone.
    pub timezone: Option<Vec<u8>>,
    /// Overlap policy of a recurring schedule.
    pub overlap: Option<Overlap>,
    /// Base retry of `queue_after_previous`.
    pub overlap_retry_ms: Option<u64>,
    /// Most fires.
    pub max_fires: Option<u64>,
    /// No fire after this time.
    pub end_at_ms: Option<u64>,
    /// When it last fired.
    pub last_fire_at_ms: Option<u64>,
    /// The target it last created.
    pub last_target_id: Option<Vec<u8>>,
    /// When an overlap last held it back.
    pub last_overlap_at_ms: Option<u64>,
    /// The target that was still active then.
    pub last_overlap_target_id: Option<Vec<u8>>,
    /// Why.
    pub last_overlap_reason: Option<Vec<u8>>,
    /// When it last skipped.
    pub last_skipped_at_ms: Option<u64>,
    /// Skips so far.
    pub skipped_count: u64,
    /// Missed periods coalesced into fires so far.
    pub coalesced_count: u64,
    /// When it last caught up.
    pub last_catchup_at_ms: Option<u64>,
    /// Periods the last catch-up coalesced.
    pub last_coalesced_count: u64,
    /// Why the last plan failed.
    pub last_planning_error: Option<Vec<u8>>,
    /// The occurrence a queued overlap still owes.
    pub overlap_queued_due_at_ms: Option<u64>,
    /// Consecutive queued retries of that occurrence.
    pub overlap_retry_streak: Option<u64>,
    /// Why it ended.
    pub end_reason: Option<Vec<u8>>,
}

fn int(value: Option<u64>) -> Term {
    Term::optional_int(value)
}

fn bin(value: Option<&Vec<u8>>) -> Term {
    Term::optional_bin(value.map(Vec::as_slice))
}

fn read_int(term: &Term, key: &str) -> Option<u64> {
    match term.get(key.as_bytes()) {
        Some(Term::Int(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn read_bin(term: &Term, key: &str) -> Option<Vec<u8>> {
    match term.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Some(value.clone()),
        _ => None,
    }
}

impl Definition {
    /// The stored map.
    #[must_use]
    pub fn term(&self) -> Term {
        map(vec![
            ("coalesced_count", Term::int(self.coalesced_count)),
            ("created_at_ms", Term::int(self.created_at_ms)),
            ("cron", bin(self.cron.as_ref())),
            ("delay_ms", int(self.delay_ms)),
            ("end_at_ms", int(self.end_at_ms)),
            ("end_reason", bin(self.end_reason.as_ref())),
            ("every_ms", int(self.every_ms)),
            ("fire_count", Term::int(self.fire_count)),
            ("id", Term::Bin(self.id.clone())),
            ("initial_run_at_ms", Term::int(self.initial_run_at_ms)),
            ("kind", Term::text(self.kind.name())),
            ("last_catchup_at_ms", int(self.last_catchup_at_ms)),
            ("last_coalesced_count", Term::int(self.last_coalesced_count)),
            ("last_fire_at_ms", int(self.last_fire_at_ms)),
            ("last_overlap_at_ms", int(self.last_overlap_at_ms)),
            (
                "last_overlap_reason",
                bin(self.last_overlap_reason.as_ref()),
            ),
            (
                "last_overlap_target_id",
                bin(self.last_overlap_target_id.as_ref()),
            ),
            (
                "last_planning_error",
                bin(self.last_planning_error.as_ref()),
            ),
            ("last_skipped_at_ms", int(self.last_skipped_at_ms)),
            ("last_target_id", bin(self.last_target_id.as_ref())),
            ("max_fires", int(self.max_fires)),
            ("next_run_at_ms", int(self.next_run_at_ms)),
            (
                "overlap_policy",
                self.overlap
                    .map_or(Term::Nil, |policy| Term::text(policy.name())),
            ),
            (
                "overlap_queued_due_at_ms",
                int(self.overlap_queued_due_at_ms),
            ),
            ("overlap_retry_ms", int(self.overlap_retry_ms)),
            ("overlap_retry_streak", int(self.overlap_retry_streak)),
            ("secret", Term::Bin(self.secret.clone())),
            ("skipped_count", Term::int(self.skipped_count)),
            ("start_at_ms", int(self.start_at_ms)),
            ("target", self.target.clone()),
            ("timezone", bin(self.timezone.as_ref())),
        ])
    }

    /// The stored bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // Every field is small; a definition past `u32::MAX` cannot exist.
        let _encoded = self.term().encode(&mut out);
        out
    }

    /// A definition from its stored bytes.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (term, _) = Term::decode(bytes)?;
        let kind = Kind::parse(&read_bin(&term, "kind")?)?;
        Some(Self {
            id: read_bin(&term, "id")?,
            kind,
            target: term
                .get(b"target")
                .cloned()
                .unwrap_or(Term::Map(Vec::new())),
            secret: read_bin(&term, "secret").unwrap_or_default(),
            created_at_ms: read_int(&term, "created_at_ms").unwrap_or(0),
            initial_run_at_ms: read_int(&term, "initial_run_at_ms").unwrap_or(0),
            next_run_at_ms: read_int(&term, "next_run_at_ms"),
            fire_count: read_int(&term, "fire_count").unwrap_or(0),
            every_ms: read_int(&term, "every_ms"),
            delay_ms: read_int(&term, "delay_ms"),
            start_at_ms: read_int(&term, "start_at_ms"),
            cron: read_bin(&term, "cron"),
            timezone: read_bin(&term, "timezone"),
            overlap: read_bin(&term, "overlap_policy").and_then(|name| Overlap::parse(&name)),
            overlap_retry_ms: read_int(&term, "overlap_retry_ms"),
            max_fires: read_int(&term, "max_fires"),
            end_at_ms: read_int(&term, "end_at_ms"),
            last_fire_at_ms: read_int(&term, "last_fire_at_ms"),
            last_target_id: read_bin(&term, "last_target_id"),
            last_overlap_at_ms: read_int(&term, "last_overlap_at_ms"),
            last_overlap_target_id: read_bin(&term, "last_overlap_target_id"),
            last_overlap_reason: read_bin(&term, "last_overlap_reason"),
            last_skipped_at_ms: read_int(&term, "last_skipped_at_ms"),
            skipped_count: read_int(&term, "skipped_count").unwrap_or(0),
            coalesced_count: read_int(&term, "coalesced_count").unwrap_or(0),
            last_catchup_at_ms: read_int(&term, "last_catchup_at_ms"),
            last_coalesced_count: read_int(&term, "last_coalesced_count").unwrap_or(0),
            last_planning_error: read_bin(&term, "last_planning_error"),
            overlap_queued_due_at_ms: read_int(&term, "overlap_queued_due_at_ms"),
            overlap_retry_streak: read_int(&term, "overlap_retry_streak"),
            end_reason: read_bin(&term, "end_reason"),
        })
    }

    /// A text field of the target.
    #[must_use]
    pub fn target_text(&self, key: &str) -> Option<&[u8]> {
        match self.target.get(key.as_bytes()) {
            Some(Term::Bin(value)) => Some(value),
            _ => None,
        }
    }

    /// The view map `FLOW.SCHEDULE.GET` and the other commands answer
    /// with; `record` is the stored Flow's `(flow id, state, version,
    /// updated time, attempts)`, version and time absent for a definition
    /// that was not read back.
    #[must_use]
    pub fn view(&self, record: (&[u8], &[u8], Option<u64>, Option<u64>, u64)) -> Term {
        let (flow_id, state, version, updated_at, attempts) = record;
        let terminal = matches!(state, b"completed" | b"failed" | b"cancelled");
        let catchup = (self.kind == Kind::Interval).then(|| Term::text("fire_once"));
        map(vec![
            ("attempts", Term::int(attempts)),
            ("catchup_policy", catchup.unwrap_or(Term::Nil)),
            ("coalesced_count", Term::int(self.coalesced_count)),
            ("created_at_ms", Term::int(self.created_at_ms)),
            ("cron", bin(self.cron.as_ref())),
            ("delay_ms", int(self.delay_ms)),
            ("end_at_ms", int(self.end_at_ms)),
            ("end_reason", bin(self.end_reason.as_ref())),
            ("every_ms", int(self.every_ms)),
            ("fire_count", Term::int(self.fire_count)),
            ("flow_id", Term::Bin(flow_id.to_vec())),
            ("id", Term::Bin(self.id.clone())),
            ("initial_run_at_ms", Term::int(self.initial_run_at_ms)),
            ("kind", Term::text(self.kind.name())),
            ("last_catchup_at_ms", int(self.last_catchup_at_ms)),
            ("last_coalesced_count", Term::int(self.last_coalesced_count)),
            ("last_fire_at_ms", int(self.last_fire_at_ms)),
            ("last_overlap_at_ms", int(self.last_overlap_at_ms)),
            (
                "last_overlap_reason",
                bin(self.last_overlap_reason.as_ref()),
            ),
            (
                "last_overlap_target_id",
                bin(self.last_overlap_target_id.as_ref()),
            ),
            (
                "last_planning_error",
                bin(self.last_planning_error.as_ref()),
            ),
            ("last_skipped_at_ms", int(self.last_skipped_at_ms)),
            ("last_target_id", bin(self.last_target_id.as_ref())),
            ("max_fires", int(self.max_fires)),
            (
                "next_run_at_ms",
                if terminal {
                    Term::Nil
                } else {
                    int(self.next_run_at_ms)
                },
            ),
            (
                "overlap_policy",
                Term::text(self.overlap.unwrap_or(Overlap::Allow).name()),
            ),
            (
                "overlap_queued_due_at_ms",
                int(self.overlap_queued_due_at_ms),
            ),
            ("overlap_retry_ms", int(self.overlap_retry_ms)),
            ("skipped_count", Term::int(self.skipped_count)),
            ("start_at_ms", int(self.start_at_ms)),
            ("state", Term::Bin(state.to_vec())),
            ("target", self.target.clone()),
            ("timezone", bin(self.timezone.as_ref())),
            ("updated_at_ms", int(updated_at)),
            ("version", int(version)),
        ])
    }
}
