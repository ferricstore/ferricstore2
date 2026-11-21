//! What one fire of a claimed schedule does, as `Schedule.fire_one/3` and
//! `fire_manual_one/4` plan it: create the target and reschedule or
//! complete the schedule, or, while the previous target still runs, skip,
//! queue or fail by the overlap policy. Automatic interval fires coalesce
//! missed periods (`fire_once` catch-up); manual fires step from the fire
//! time.

use super::super::opts::MAX_EXACT;
use super::super::term::{Term, map};
use super::cron;
use super::definition::{Definition, Kind, Overlap};
use super::target;

mod overlap;

use overlap::{fail, queue, skip};

const DEFAULT_ZONE: &str = "Etc/UTC";

/// How the claimed schedule record changes.
#[derive(Clone, Debug, PartialEq)]
pub enum Apply {
    /// Back to `active`, due at `run_at`.
    Reschedule {
        /// The new definition.
        definition: Definition,
        /// The next due time.
        run_at: u64,
    },
    /// `completed`, with a result.
    Complete {
        /// The final definition.
        definition: Definition,
        /// The result value.
        result: Term,
    },
    /// `failed`, with an error.
    Fail {
        /// The final definition.
        definition: Definition,
        /// The error value.
        error: Term,
    },
}

/// One fire's plan.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Create `target_id`, then apply.
    Fire {
        /// The target to create.
        target_id: Vec<u8>,
        /// The schedule change once it exists.
        then: Apply,
        /// Missed periods coalesced.
        coalesced: u64,
    },
    /// Hold back this occurrence (skip or queue).
    Hold {
        /// Why.
        reason: String,
        /// The schedule change.
        then: Apply,
        /// Missed periods coalesced.
        coalesced: u64,
    },
    /// Fail the schedule and report `reason`.
    Fail {
        /// Why.
        reason: String,
        /// The schedule change.
        then: Apply,
    },
}

/// Why stepping a schedule failed.
pub(super) enum Next {
    /// A next due time and the periods coalesced.
    At(u64, u64),
    /// The recurrence ended at its `end_at_ms`.
    EndAt(u64),
    /// The next time would pass the exact-integer limit.
    Limit(u64),
}

pub(super) fn add(left: u64, right: u64) -> Option<u64> {
    left.checked_add(right).filter(|sum| *sum <= MAX_EXACT)
}

/// The next due time after an occurrence due at `due`; automatic interval
/// fires coalesce what was missed up to `now`.
pub(super) fn next(definition: &Definition, due: u64, now: Option<u64>) -> Result<Next, String> {
    match (definition.kind, definition.every_ms, now) {
        (Kind::Delay, ..) => Ok(Next::At(due, 0)),
        (Kind::Interval, Some(every), Some(now)) => {
            let horizon = definition.end_at_ms.map_or(now, |end| end.min(now));
            let elapsed = horizon.saturating_sub(due);
            let coalesced = if elapsed < every {
                0
            } else {
                elapsed.checked_div(every).unwrap_or(0)
            };
            if definition.end_at_ms.is_some_and(|end| end <= now) {
                return Ok(Next::EndAt(coalesced));
            }
            let base = if coalesced > 0 { now } else { due };
            Ok(add(base, every).map_or(Next::Limit(coalesced), |next| Next::At(next, coalesced)))
        }
        (Kind::Interval, Some(every), None) => {
            Ok(add(due, every).map_or(Next::Limit(0), |next| Next::At(next, 0)))
        }
        (Kind::Cron, ..) => {
            let expression =
                String::from_utf8_lossy(definition.cron.as_deref().unwrap_or_default())
                    .into_owned();
            let zone = definition.timezone.as_deref().map_or_else(
                || DEFAULT_ZONE.to_owned(),
                |zone| String::from_utf8_lossy(zone).into_owned(),
            );
            match cron::next_run_at_ms(&expression, i64::try_from(due).unwrap_or(i64::MAX), &zone) {
                Ok(next) if next <= MAX_EXACT => Ok(Next::At(next, 0)),
                Ok(_) => Ok(Next::Limit(0)),
                Err(error) if error == cron::CALENDAR_RANGE => Ok(Next::Limit(0)),
                Err(error) => Err(error),
            }
        }
        _ => Err("ERR schedule payload missing".to_owned()),
    }
}

pub(super) fn catchup(mut definition: Definition, now: u64, coalesced: u64) -> Definition {
    if coalesced > 0 {
        definition.coalesced_count = definition
            .coalesced_count
            .saturating_add(coalesced)
            .min(MAX_EXACT);
        definition.last_catchup_at_ms = Some(now);
        definition.last_coalesced_count = coalesced;
    }
    definition
}

fn after_fire(
    mut definition: Definition,
    target_id: &[u8],
    due: u64,
    count: u64,
    next: Option<u64>,
) -> Definition {
    definition.fire_count = count;
    definition.last_fire_at_ms = Some(due);
    definition.last_target_id = Some(target_id.to_vec());
    definition.next_run_at_ms = next;
    definition.overlap_queued_due_at_ms = None;
    definition.last_overlap_at_ms = None;
    definition.last_overlap_target_id = None;
    definition.last_overlap_reason = None;
    definition.overlap_retry_streak = None;
    definition.end_reason = None;
    definition
}

pub(super) fn ended(mut definition: Definition, reason: &str) -> Definition {
    definition.end_reason = Some(reason.as_bytes().to_vec());
    definition.next_run_at_ms = None;
    definition.overlap_queued_due_at_ms = None;
    definition.overlap_retry_streak = None;
    definition
}

pub(super) fn end_reason(
    definition: &Definition,
    next_run: u64,
    count: u64,
) -> Option<&'static str> {
    if definition.max_fires.is_some_and(|max| count >= max) {
        Some("max_fires")
    } else if definition.end_at_ms.is_some_and(|end| next_run > end) {
        Some("end_at_ms")
    } else {
        None
    }
}

fn fire_result(target_id: &[u8], count: u64, due: u64, reason: Option<&str>) -> Term {
    let mut entries = vec![
        ("fire_count", Term::int(count)),
        ("fired_at_ms", Term::int(due)),
        ("target_id", Term::Bin(target_id.to_vec())),
    ];
    entries.extend(reason.map(|reason| ("end_reason", Term::text(reason))));
    entries.sort_by_key(|(key, _)| *key);
    map(entries)
}

/// `completed_definition_after_fire/5`: the fire recorded and the schedule
/// ended, overlap details kept.
fn completed_after_fire(
    mut definition: Definition,
    target_id: &[u8],
    due: u64,
    count: u64,
    reason: &str,
) -> Apply {
    let result = fire_result(target_id, count, due, Some(reason));
    definition.fire_count = count;
    definition.last_fire_at_ms = Some(due);
    definition.last_target_id = Some(target_id.to_vec());
    let definition = ended(definition, reason);
    Apply::Complete { definition, result }
}

fn complete_fire(
    definition: Definition,
    target_id: &[u8],
    due: u64,
    count: u64,
    reason: &str,
) -> Apply {
    let result = fire_result(target_id, count, due, Some(reason));
    let definition = ended(after_fire(definition, target_id, due, count, None), reason);
    Apply::Complete { definition, result }
}

/// The plan for firing when the previous target is free.
fn fire(definition: Definition, due: u64, now: u64, manual: bool) -> Result<Plan, String> {
    let count = definition.fire_count.saturating_add(1);
    let target_id = target::target_id(&definition, due, count);
    if matches!(definition.kind, Kind::OneShot | Kind::Delay) {
        let result = fire_result(&target_id, count, due, None);
        let definition = ended(
            after_fire(definition, &target_id, due, count, None),
            "one_shot_fired",
        );
        let then = Apply::Complete { definition, result };
        return Ok(Plan::Fire {
            target_id,
            then,
            coalesced: 0,
        });
    }
    let (then, coalesced) = match next(&definition, due, (!manual).then_some(now))? {
        Next::At(run_at, coalesced) => {
            let next_definition = catchup(
                after_fire(definition, &target_id, due, count, Some(run_at)),
                now,
                coalesced,
            );
            match end_reason(&next_definition, run_at, count) {
                None => (
                    Apply::Reschedule {
                        definition: next_definition,
                        run_at,
                    },
                    coalesced,
                ),
                Some(reason) => (
                    complete_fire(next_definition, &target_id, due, count, reason),
                    coalesced,
                ),
            }
        }
        Next::EndAt(coalesced) => {
            let reason = if definition.max_fires.is_some_and(|max| count >= max) {
                "max_fires"
            } else {
                "end_at_ms"
            };
            (
                completed_after_fire(
                    catchup(definition, now, coalesced),
                    &target_id,
                    due,
                    count,
                    reason,
                ),
                coalesced,
            )
        }
        Next::Limit(coalesced) => (
            completed_after_fire(
                catchup(definition, now, coalesced),
                &target_id,
                due,
                count,
                "timestamp_limit",
            ),
            coalesced,
        ),
    };
    Ok(Plan::Fire {
        target_id,
        then,
        coalesced,
    })
}

/// The plan for one fire of `definition`: automatic when `fire_at` is
/// `None` (due at its queued or next time), manual at `fire_at` otherwise;
/// `previous` is the last target's id while it is still active.
///
/// # Errors
/// A planning failure's text, such as a cron with no next match.
pub fn plan(
    definition: Definition,
    fire_at: Option<u64>,
    now: u64,
    previous: Option<&[u8]>,
) -> Result<Plan, String> {
    let manual = fire_at.is_some();
    let due = fire_at
        .or(definition.overlap_queued_due_at_ms)
        .or(definition.next_run_at_ms)
        .unwrap_or(now);
    let policy = definition.overlap.unwrap_or(Overlap::Allow);
    let Some(previous) = previous.filter(|_| policy != Overlap::Allow) else {
        return fire(definition, due, now, manual);
    };
    let reason = format!(
        "previous target still active: {}",
        String::from_utf8_lossy(previous)
    );
    Ok(match policy {
        Overlap::Skip => {
            let (then, coalesced) = skip(definition, due, &reason, now, manual)?;
            Plan::Hold {
                reason,
                then,
                coalesced,
            }
        }
        Overlap::QueueAfterPrevious => {
            let then = queue(definition, due, &reason, now, manual);
            Plan::Hold {
                reason,
                then,
                coalesced: 0,
            }
        }
        Overlap::FailSchedule | Overlap::Allow => {
            let then = fail(definition, &reason, now);
            Plan::Fail { reason, then }
        }
    })
}
