//! `FLOW.SCHEDULE.CREATE`'s definition, checked as `Schedule.definition/2`
//! checks it, in the same order and with the same texts.

use super::super::FlowError;
use super::super::opts::MAX_EXACT;
use super::super::term::Term;
use super::cron;
use super::definition::{Definition, Kind, Overlap};
use super::target;

mod target_options;

use super::Request;
use target_options::{normalize_target, target_rules};

const DEFAULT_ZONE: &str = "Etc/UTC";

fn rejected(text: impl Into<String>) -> FlowError {
    FlowError::rejected(text.into())
}

fn exceeds(key: &str) -> FlowError {
    rejected(format!(
        "ERR flow schedule {key} exceeds maximum {MAX_EXACT}"
    ))
}

/// `optional_non_neg_integer/3` of a value.
fn non_neg(value: Option<&Term>, key: &str) -> Result<Option<u64>, FlowError> {
    match value {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Int(number)) => match u64::try_from(*number) {
            Ok(number) if number <= MAX_EXACT => Ok(Some(number)),
            Ok(_) => Err(exceeds(key)),
            Err(_) => Err(rejected(format!(
                "ERR flow schedule {key} must be a non-negative integer"
            ))),
        },
        Some(_) => Err(rejected(format!(
            "ERR flow schedule {key} must be a non-negative integer"
        ))),
    }
}

/// A positive integer option, `None` when absent.
fn positive(value: Option<&Term>, key: &str) -> Result<Option<u64>, FlowError> {
    match value {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Int(number)) if *number > 0 => match u64::try_from(*number) {
            Ok(number) if number <= MAX_EXACT => Ok(Some(number)),
            _ => Err(exceeds(key)),
        },
        Some(_) => Err(rejected(format!(
            "ERR flow schedule {key} must be a positive integer"
        ))),
    }
}

fn add(left: u64, right: u64, key: &str) -> Result<u64, FlowError> {
    left.checked_add(right)
        .filter(|sum| *sum <= MAX_EXACT)
        .ok_or_else(|| exceeds(key))
}

fn kind(options: &Term) -> Result<Kind, FlowError> {
    match options.get(b"kind") {
        None | Some(Term::Nil) => Ok(if options.get(b"cron").is_some() {
            Kind::Cron
        } else if options.get(b"every_ms").is_some() {
            Kind::Interval
        } else if options.get(b"delay_ms").is_some() {
            Kind::Delay
        } else {
            Kind::OneShot
        }),
        Some(Term::Bin(name)) => Kind::parse(name).ok_or_else(|| {
            rejected("ERR flow schedule kind must be :one_shot, :delay, :interval, or :cron")
        }),
        Some(_) => Err(rejected(
            "ERR flow schedule kind must be :one_shot, :delay, :interval, or :cron",
        )),
    }
}

fn timing(kind: Kind, options: &Term) -> Result<(), FlowError> {
    if options.get(b"at_ms").is_some() && options.get(b"start_at_ms").is_some() {
        return Err(rejected(
            "ERR flow schedule cannot set both at_ms and start_at_ms",
        ));
    }
    let start_kinds = [Kind::OneShot, Kind::Interval, Kind::Cron];
    for (key, kinds, label) in [
        ("delay_ms", &[Kind::Delay][..], "delay"),
        ("every_ms", &[Kind::Interval][..], "interval"),
        ("cron", &[Kind::Cron][..], "cron"),
        ("at_ms", &start_kinds[..], "one_shot, interval, or cron"),
        (
            "start_at_ms",
            &start_kinds[..],
            "one_shot, interval, or cron",
        ),
    ] {
        if options.get(key.as_bytes()).is_some() && !kinds.contains(&kind) {
            return Err(rejected(format!(
                "ERR flow schedule {key} is only supported for {label} schedules"
            )));
        }
    }
    Ok(())
}

fn zone(kind: Kind, options: &Term) -> Result<Option<Vec<u8>>, FlowError> {
    if kind != Kind::Cron {
        return match options.get(b"timezone") {
            Some(_) => Err(rejected(
                "ERR flow schedule timezone is only supported for cron schedules",
            )),
            None => Ok(None),
        };
    }
    let name = match options.get(b"timezone") {
        None => DEFAULT_ZONE.as_bytes().to_vec(),
        Some(Term::Bin(name)) if name.as_slice() == b"UTC" => DEFAULT_ZONE.as_bytes().to_vec(),
        Some(Term::Bin(name)) if !name.is_empty() => name.clone(),
        Some(_) => {
            return Err(rejected(
                "ERR flow schedule timezone must be a non-empty string",
            ));
        }
    };
    cron::zone(&String::from_utf8_lossy(&name)).map_err(rejected)?;
    Ok(Some(name))
}

fn expression(options: &Term) -> Result<String, FlowError> {
    match options.get(b"cron") {
        Some(Term::Bin(text)) if !text.is_empty() => {
            let text = String::from_utf8_lossy(text).into_owned();
            cron::parse(&text).map_err(rejected)?;
            Ok(text)
        }
        _ => Err(rejected(
            "ERR flow schedule cron must be a non-empty string",
        )),
    }
}

/// The first due time.
fn first_run(
    kind: Kind,
    options: &Term,
    now_ms: u64,
    zone: Option<&[u8]>,
) -> Result<u64, FlowError> {
    let at = options.get(b"at_ms");
    let start = options.get(b"start_at_ms");
    match kind {
        Kind::Delay => match options.get(b"delay_ms") {
            Some(Term::Int(delay)) if *delay >= 0 => {
                let delay = u64::try_from(*delay).unwrap_or(u64::MAX);
                if delay > MAX_EXACT {
                    return Err(exceeds("delay_ms"));
                }
                add(now_ms, delay, "at_ms")
            }
            _ => Err(rejected(
                "ERR flow schedule delay_ms must be a non-negative integer",
            )),
        },
        Kind::Cron => {
            let expression = expression(options)?;
            let start = non_neg(start.or(at), "start_at_ms")?.unwrap_or(now_ms);
            let zone =
                String::from_utf8_lossy(zone.unwrap_or(DEFAULT_ZONE.as_bytes())).into_owned();
            let after = i64::try_from(start).unwrap_or(i64::MAX).saturating_sub(1);
            cron::next_run_at_ms(&expression, after, &zone).map_err(rejected)
        }
        Kind::OneShot | Kind::Interval => Ok(non_neg(at.or(start), "at_ms")?.unwrap_or(now_ms)),
    }
}

fn overlap(kind: Kind, options: &Term) -> Result<Option<Overlap>, FlowError> {
    let given = options.get(b"overlap_policy");
    if !kind.recurring() {
        return match given {
            Some(_) => Err(rejected(
                "ERR flow schedule overlap_policy is only supported for recurring schedules",
            )),
            None => Ok(None),
        };
    }
    match given {
        None => Ok(Some(Overlap::Allow)),
        Some(Term::Bin(name)) if Overlap::parse(name).is_some() => Ok(Overlap::parse(name)),
        Some(_) => Err(rejected(
            "ERR flow schedule overlap_policy must be :allow, :skip, :queue_after_previous, or :fail_schedule",
        )),
    }
}

fn recurring_only(kind: Kind, options: &Term, key: &str) -> Result<(), FlowError> {
    if !kind.recurring() && options.get(key.as_bytes()).is_some() {
        return Err(rejected(format!(
            "ERR flow schedule {key} is only supported for recurring schedules"
        )));
    }
    Ok(())
}

/// The catch-up and overlap policies, checked as Elixir does.
fn policies(kind: Kind, options: &Term) -> Result<(Option<Overlap>, Option<u64>), FlowError> {
    match (kind, options.get(b"catchup_policy")) {
        (Kind::Interval, Some(Term::Bin(name))) if name.as_slice() == b"fire_once" => {}
        (Kind::Interval, Some(Term::Nil)) | (_, None) => {}
        (Kind::Interval, Some(_)) => {
            return Err(rejected(
                "ERR flow schedule catchup_policy must be :fire_once",
            ));
        }
        (_, Some(_)) => {
            return Err(rejected(
                "ERR flow schedule catchup_policy is only supported for interval schedules",
            ));
        }
    }
    let overlap = overlap(kind, options)?;
    let retry = match positive(options.get(b"overlap_retry_ms"), "overlap_retry_ms")? {
        Some(retry) if kind.recurring() && overlap == Some(Overlap::QueueAfterPrevious) => {
            Some(retry)
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow schedule overlap_retry_ms requires overlap_policy :queue_after_previous",
            ));
        }
        None => None,
    };
    Ok((overlap, retry))
}

/// A new definition with no fires and nothing optional set.
fn blank(
    id: Vec<u8>,
    kind: Kind,
    target: Term,
    (now_ms, next_run_at_ms): (u64, u64),
) -> Definition {
    Definition {
        id,
        kind,
        target,
        secret: Vec::new(),
        created_at_ms: now_ms,
        initial_run_at_ms: next_run_at_ms,
        next_run_at_ms: Some(next_run_at_ms),
        fire_count: 0,
        every_ms: None,
        delay_ms: None,
        start_at_ms: None,
        cron: None,
        timezone: None,
        overlap: None,
        overlap_retry_ms: None,
        max_fires: None,
        end_at_ms: None,
        last_fire_at_ms: None,
        last_target_id: None,
        last_overlap_at_ms: None,
        last_overlap_target_id: None,
        last_overlap_reason: None,
        last_skipped_at_ms: None,
        skipped_count: 0,
        coalesced_count: 0,
        last_catchup_at_ms: None,
        last_coalesced_count: 0,
        last_planning_error: None,
        overlap_queued_due_at_ms: None,
        overlap_retry_streak: None,
        end_reason: None,
    }
}

/// The definition `request` creates at `now_ms`, without its ownership
/// secret, which the node adds.
///
/// # Errors
/// Elixir's text for the first invalid option.
pub fn definition(request: &Request, wall_ms: u64) -> Result<Definition, FlowError> {
    let options = &request.options;
    let now_ms = match options.get(b"now_ms") {
        None => wall_ms,
        value => non_neg(value, "now_ms")?.unwrap_or(wall_ms),
    };
    let kind = kind(options)?;
    timing(kind, options)?;
    let target = normalize_target(options)?;
    target_rules(kind, &target)?;
    let timezone = zone(kind, options)?;
    let next_run_at_ms = first_run(kind, options, now_ms, timezone.as_deref())?;
    let every_ms = if kind == Kind::Interval {
        match positive(options.get(b"every_ms"), "every_ms")? {
            Some(every) => Some(every),
            None => {
                return Err(rejected(
                    "ERR flow schedule every_ms must be a positive integer",
                ));
            }
        }
    } else {
        None
    };
    let cron = if kind == Kind::Cron {
        Some(expression(options)?.into_bytes())
    } else {
        None
    };
    let (overlap, overlap_retry_ms) = policies(kind, options)?;
    recurring_only(kind, options, "max_fires")?;
    let max_fires = positive(options.get(b"max_fires"), "max_fires")?;
    recurring_only(kind, options, "end_at_ms")?;
    let end_at_ms = non_neg(options.get(b"end_at_ms"), "end_at_ms")?;
    if end_at_ms.is_some_and(|end| next_run_at_ms > end) {
        return Err(rejected(
            "ERR flow schedule end_at_ms must be at or after first run",
        ));
    }
    let start_at_ms = if kind.recurring() {
        non_neg(options.get(b"start_at_ms"), "start_at_ms")?
    } else {
        None
    };
    let definition = Definition {
        every_ms,
        delay_ms: (kind == Kind::Delay).then(|| next_run_at_ms.saturating_sub(now_ms)),
        start_at_ms,
        cron,
        timezone,
        overlap,
        overlap_retry_ms,
        max_fires,
        end_at_ms,
        ..blank(request.id.clone(), kind, target, (now_ms, next_run_at_ms))
    };
    // The first and the longest target ids must both make valid Flows.
    for (due, count) in [(next_run_at_ms, 1), (MAX_EXACT, MAX_EXACT)] {
        let id = target::target_id(&definition, due, count);
        super::super::parse(
            "FLOW.CREATE",
            &target::create_options(&definition, &id, now_ms),
        )?;
    }
    Ok(definition)
}
