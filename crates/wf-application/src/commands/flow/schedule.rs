//! `FLOW.SCHEDULE.*` (ADR-014 step 13), as `Ferricstore.Flow.Schedule`
//! defines them.
//!
//! A schedule is an internal Flow of type `__ferricstore_schedule`, id
//! `__ferricstore_schedule__:<id>`, in one of 256 internal partitions; its
//! payload is the definition. A fire claims the schedule, creates its
//! target Flow (an idempotent id, owned through an attribute digest),
//! then reschedules or completes it; while the previous target still runs,
//! a recurring schedule may skip, queue or fail instead. The node fires due
//! schedules in the background and on `FLOW.SCHEDULE.FIRE_DUE`.

pub mod create;
pub mod cron;
#[cfg(test)]
mod cron_tests;
pub mod definition;
pub mod guard;
pub mod list;
pub mod plan;
pub mod target;
#[cfg(test)]
mod tests;

use super::FlowError;
use super::opts::{MAX_EXACT, Opts};
use super::term::Term;
use crate::routing::Partition;
use guard::{Expect, options};

/// The internal Flow type of schedules.
pub const SCHEDULE_TYPE: &str = "__ferricstore_schedule";
/// The prefix of schedule Flow ids, reserved for internal use.
pub const FLOW_ID_PREFIX: &str = "__ferricstore_schedule__:";
/// Elixir's default fire lease.
pub const DEFAULT_LEASE_MS: u64 = 30_000;
/// Elixir's default `FIRE_DUE` limit.
pub const DEFAULT_LIMIT: u64 = 100;
/// The schedules' own history window.
pub const HISTORY_MAX_EVENTS: u64 = 100;

/// A command naming one schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The schedule id.
    pub id: Vec<u8>,
    /// Command time, when given.
    pub now_ms: Option<u64>,
    /// The state and version the caller read.
    pub expect: Expect,
}

/// `FLOW.SCHEDULE.CREATE`, read when it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The schedule id.
    pub id: Vec<u8>,
    /// Replace an existing schedule.
    pub overwrite: bool,
    /// With `overwrite`, the state and version the caller read.
    pub expect: Expect,
    /// The options.
    pub options: Term,
}

/// `FLOW.SCHEDULE.FIRE_DUE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FireDue {
    /// Command time.
    pub now_ms: Option<u64>,
    /// Most schedules fired.
    pub limit: u64,
    /// The worker named on the leases.
    pub worker: Vec<u8>,
    /// Lease length.
    pub lease_ms: u64,
}

/// A schedule command.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `FLOW.SCHEDULE.CREATE`.
    Create(Request),
    /// `FLOW.SCHEDULE.GET`.
    Get(Target),
    /// `FLOW.SCHEDULE.DELETE`.
    Delete(Target),
    /// `FLOW.SCHEDULE.PAUSE`.
    Pause(Target),
    /// `FLOW.SCHEDULE.RESUME`.
    Resume(Target),
    /// `FLOW.SCHEDULE.FIRE`, at `fire_at_ms` or now.
    Fire(Target, Option<u64>),
    /// `FLOW.SCHEDULE.FIRE_DUE`.
    FireDue(FireDue),
    /// `FLOW.SCHEDULE.LIST`.
    List(list::List),
}

/// The Flow id of schedule `id`.
#[must_use]
pub fn flow_id(id: &[u8]) -> Vec<u8> {
    let mut flow_id = FLOW_ID_PREFIX.as_bytes().to_vec();
    flow_id.extend_from_slice(id);
    flow_id
}

/// The internal partition of schedule `id`: one of 256 buckets.
#[must_use]
pub fn partition(id: &[u8]) -> Partition {
    let Partition::Auto(bucket) = Partition::for_new_flow(id, None) else {
        return Partition::Named(FLOW_ID_PREFIX.as_bytes().to_vec());
    };
    let mut name = FLOW_ID_PREFIX.as_bytes().to_vec();
    name.extend_from_slice(bucket.to_string().as_bytes());
    Partition::Named(name)
}

fn rejected(text: impl Into<String>) -> FlowError {
    FlowError::rejected(text.into())
}

/// The schedule id: a binary (the native check), not empty.
fn id(opts: &Opts<'_>) -> Result<Vec<u8>, FlowError> {
    match opts.get("id") {
        Some(Term::Bin(id)) if id.is_empty() => {
            Err(rejected("ERR flow schedule id must be a non-empty string"))
        }
        Some(Term::Bin(id)) => Ok(id.clone()),
        _ => Err(rejected("ERR native field id must be binary")),
    }
}

/// `optional_now_ms/1` and the other schedule integers.
pub(super) fn integer(opts: &Opts<'_>, key: &str) -> Result<Option<u64>, FlowError> {
    match opts.get(key) {
        None => Ok(None),
        Some(Term::Int(value)) => match u64::try_from(*value) {
            Ok(value) if value <= MAX_EXACT => Ok(Some(value)),
            Ok(_) => Err(rejected(format!(
                "ERR flow schedule {key} exceeds maximum {MAX_EXACT}"
            ))),
            Err(_) => Err(rejected(format!(
                "ERR flow schedule {key} must be a non-negative integer"
            ))),
        },
        Some(_) => Err(rejected(format!(
            "ERR flow schedule {key} must be a non-negative integer"
        ))),
    }
}

fn simple(name: &str, args: &Term, extra: &[&str]) -> Result<(Target, Option<u64>), FlowError> {
    let mut allowed = vec!["id", "now_ms", "deadline_ms"];
    allowed.extend_from_slice(&Expect::KEYS);
    allowed.extend_from_slice(extra);
    let opts = options(name, args, &allowed)?;
    let id = id(&opts)?;
    let now_ms = integer(&opts, "now_ms")?;
    let fire_at_ms = if extra.contains(&"fire_at_ms") {
        integer(&opts, "fire_at_ms")?
    } else {
        None
    };
    let expect = Expect::read(&opts)?;
    Ok((Target { id, now_ms, expect }, fire_at_ms))
}

/// `FLOW.SCHEDULE.CREATE`: the id and `overwrite` now, the options when
/// it runs.
fn parse_create(name: &str, args: &Term) -> Result<Command, FlowError> {
    let opts = options(
        name,
        args,
        &[
            "id",
            "kind",
            "target",
            "at_ms",
            "delay_ms",
            "start_at_ms",
            "every_ms",
            "cron",
            "timezone",
            "now_ms",
            "overwrite",
            "catchup_policy",
            "overlap_policy",
            "overlap_retry_ms",
            "max_fires",
            "end_at_ms",
            "expected_state",
            "expected_version",
            "deadline_ms",
        ],
    )?;
    let id = id(&opts)?;
    let expect = Expect::read(&opts)?;
    let overwrite = match opts.get("overwrite") {
        None => false,
        Some(Term::Bool(flag)) => *flag,
        Some(_) => return Err(rejected("ERR flow schedule overwrite must be a boolean")),
    };
    let entries = match args {
        Term::Map(entries) => entries.clone(),
        _ => Vec::new(),
    };
    let options = Term::Map(
        entries
            .into_iter()
            .filter(|(key, value)| {
                !matches!(
                    key.as_slice(),
                    b"id" | b"overwrite" | b"deadline_ms" | b"expected_state" | b"expected_version"
                ) && *value != Term::Nil
            })
            .collect(),
    );
    Ok(Command::Create(Request {
        id,
        overwrite,
        expect,
        options,
    }))
}

/// `FLOW.SCHEDULE.FIRE_DUE`.
fn parse_fire_due(name: &str, args: &Term) -> Result<Command, FlowError> {
    let opts = options(
        name,
        args,
        &[
            "now_ms",
            "worker",
            "limit",
            "lease_ms",
            "block_ms",
            "deadline_ms",
        ],
    )?;
    let limit = match opts.get("limit") {
        None => DEFAULT_LIMIT,
        Some(Term::Int(limit)) if *limit > 0 => u64::try_from(*limit).unwrap_or(DEFAULT_LIMIT),
        Some(_) => return Err(rejected("ERR flow limit must be a positive integer")),
    };
    let worker = match opts.get("worker") {
        None => b"ferricstore-scheduler:ferricstore2".to_vec(),
        Some(Term::Bin(worker)) if !worker.is_empty() => worker.clone(),
        Some(_) => return Err(rejected("ERR flow worker must be a non-empty string")),
    };
    let lease_ms = match opts.get("lease_ms") {
        None => DEFAULT_LEASE_MS,
        Some(Term::Int(lease)) if *lease > 0 => u64::try_from(*lease).unwrap_or(DEFAULT_LEASE_MS),
        Some(_) => return Err(rejected("ERR flow lease_ms must be a positive integer")),
    };
    Ok(Command::FireDue(FireDue {
        now_ms: integer(&opts, "now_ms")?,
        limit,
        worker,
        lease_ms,
    }))
}

/// Reads schedule command `name`.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    match name {
        "FLOW.SCHEDULE.CREATE" => parse_create(name, args),
        "FLOW.SCHEDULE.GET" => {
            // Elixir's internal routing and payload options stay its own.
            let opts = options(
                name,
                args,
                &[
                    "id",
                    "partition_key",
                    "payload",
                    "payload_max_bytes",
                    "deadline_ms",
                ],
            )?;
            Ok(Command::Get(Target {
                id: id(&opts)?,
                now_ms: None,
                expect: Expect::default(),
            }))
        }
        "FLOW.SCHEDULE.DELETE" => {
            simple(name, args, &[]).map(|(target, _)| Command::Delete(target))
        }
        "FLOW.SCHEDULE.PAUSE" => simple(name, args, &[]).map(|(target, _)| Command::Pause(target)),
        "FLOW.SCHEDULE.RESUME" => {
            simple(name, args, &[]).map(|(target, _)| Command::Resume(target))
        }
        "FLOW.SCHEDULE.FIRE" => {
            simple(name, args, &["fire_at_ms"]).map(|(target, at)| Command::Fire(target, at))
        }
        "FLOW.SCHEDULE.FIRE_DUE" => parse_fire_due(name, args),
        "FLOW.SCHEDULE.LIST" => list::parse(args).map(Command::List),
        _ => Err(rejected(format!(
            "ERR {name} is not supported by this server"
        ))),
    }
}
