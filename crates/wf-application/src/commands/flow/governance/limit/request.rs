//! Reading limit requests in the order `LimitStore` checks them. The shard
//! id is checked against the node's shard count before the command is
//! routed ([`Command::admit`]); the options checked after it are kept with
//! their outcome, and those Elixir checks only once the limit is found are
//! kept apart.

use super::super::super::opts::{MAX_EXACT, Opts};
use super::super::super::term::Term;
use super::super::super::{FlowError, keys};
use super::super::{key_size, native};
use super::fingerprint;
use crate::routing::Partition;

mod list;

pub use list::{List, parse_list};

const MAX_AMOUNT: i64 = 1_000;
const MAX_POLICY_VERSION_BYTES: usize = 262_144;
const MAX_RESERVATION_ID_BYTES: usize = 256;

pub(super) fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// What a limit command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `FLOW.LIMIT.LEASE`.
    Lease,
    /// `FLOW.LIMIT.SPEND`.
    Spend,
    /// `FLOW.LIMIT.RELEASE`.
    Release,
    /// `FLOW.LIMIT.GET`.
    Get,
}

/// A configuration a lease or spend may carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Configuration {
    /// Credits in all.
    pub limit: Option<u64>,
    /// Its version.
    pub config_version: Option<u64>,
    /// The policy version's fingerprint.
    pub policy_version: Option<[u8; 32]>,
}

/// The options checked before the limit is looked up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    /// Credits asked for, spent or released.
    pub amount: u64,
    /// How long a lease lasts.
    pub ttl_ms: Option<u64>,
    /// The client's clock.
    pub now_ms: Option<u64>,
    /// The reservation ids released.
    pub ids: Vec<Vec<u8>>,
    /// A lease's configuration.
    pub configuration: Configuration,
}

/// A command on one limit.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// What it does.
    pub op: Op,
    /// The scope.
    pub scope: Vec<u8>,
    /// The shard id as given; checked by [`Command::admit`].
    shard: Option<Term>,
    /// The shard id once admitted.
    pub shard_id: u64,
    /// The options checked before the lookup, or the first refused.
    pub fields: Result<Fields, FlowError>,
    /// A spend's options checked after the lookup, or the first refused.
    pub late: Result<Configuration, FlowError>,
}

impl Command {
    /// Checks the shard id against the node's `shards`, then the options
    /// checked after it, as Elixir does before mutating.
    ///
    /// # Errors
    /// Elixir's text for the first refused option.
    pub fn admit(&mut self, shards: usize) -> Result<(), FlowError> {
        if let Some(shard) = &self.shard {
            self.shard_id = match shard {
                Term::Int(shard) if usize::try_from(*shard).is_ok_and(|shard| shard < shards) => {
                    u64::try_from(*shard).unwrap_or(0)
                }
                _ => {
                    return Err(FlowError::rejected(format!(
                        "ERR flow limit shard_id must be between 0 and {}",
                        shards.saturating_sub(1)
                    )));
                }
            };
        }
        self.fields.as_ref().map(|_| ()).map_err(Clone::clone)
    }
}

pub(super) fn now_ms(opts: &Opts<'_>) -> Result<Option<u64>, FlowError> {
    match opts.get("now_ms") {
        None => Ok(None),
        Some(Term::Int(now)) if u64::try_from(*now).is_ok_and(|now| now <= MAX_EXACT) => {
            Ok(u64::try_from(*now).ok())
        }
        Some(_) => Err(rejected(
            "ERR flow limit now_ms must be a non-negative integer",
        )),
    }
}

fn amount(opts: &Opts<'_>) -> Result<u64, FlowError> {
    match opts.get("amount") {
        Some(Term::Int(amount)) if (1..=MAX_AMOUNT).contains(amount) => {
            Ok(u64::try_from(*amount).unwrap_or(1))
        }
        _ => Err(rejected(
            "ERR flow limit amount must be a positive integer at most 1000",
        )),
    }
}

fn ttl(opts: &Opts<'_>, required: bool) -> Result<Option<u64>, FlowError> {
    match opts.get("ttl_ms") {
        None if !required => Ok(None),
        Some(Term::Int(ttl))
            if *ttl > 0 && u64::try_from(*ttl).is_ok_and(|ttl| ttl <= MAX_EXACT) =>
        {
            Ok(u64::try_from(*ttl).ok())
        }
        _ => Err(rejected("ERR flow limit ttl_ms must be a positive integer")),
    }
}

/// `validate_deadline/2`; a node's own clock is far below the bound.
fn deadline(now: Option<u64>, ttl: Option<u64>) -> Result<(), FlowError> {
    match (now, ttl) {
        (Some(now), Some(ttl)) if now > MAX_EXACT.saturating_sub(ttl) => Err(rejected(
            "ERR flow limit lease deadline exceeds the supported integer range",
        )),
        _ => Ok(()),
    }
}

/// `limit_configuration/1`.
fn configuration(opts: &Opts<'_>) -> Result<Configuration, FlowError> {
    let limit = match opts.get("limit") {
        None => None,
        Some(Term::Int(limit)) if u64::try_from(*limit).is_ok_and(|limit| limit <= MAX_EXACT) => {
            u64::try_from(*limit).ok()
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow limit limit must be a non-negative integer",
            ));
        }
    };
    let policy = match opts.get("policy_version") {
        None => None,
        Some(version @ Term::Int(number))
            if u64::try_from(*number).is_ok_and(|number| number <= MAX_EXACT) =>
        {
            Some(version)
        }
        Some(version @ Term::Bin(text))
            if !text.is_empty() && text.len() <= MAX_POLICY_VERSION_BYTES =>
        {
            Some(version)
        }
        Some(_) => return Err(rejected("ERR flow limit policy_version is invalid")),
    };
    let config_version = match (opts.get("config_version"), policy) {
        (None, Some(Term::Int(number))) => u64::try_from(*number).ok(),
        (None, Some(_)) => {
            return Err(rejected(
                "ERR flow limit config_version is required with binary policy_version",
            ));
        }
        (None, None) => None,
        (Some(Term::Int(version)), _)
            if u64::try_from(*version).is_ok_and(|version| version <= MAX_EXACT) =>
        {
            u64::try_from(*version).ok()
        }
        (Some(_), _) => {
            return Err(rejected(
                "ERR flow limit config_version must be a non-negative integer",
            ));
        }
    };
    if limit.is_none() && (config_version.is_some() || policy.is_some()) {
        return Err(rejected(
            "ERR flow limit limit is required with configuration version",
        ));
    }
    Ok(Configuration {
        limit,
        config_version,
        policy_version: policy.and_then(fingerprint),
    })
}

fn release_ids(opts: &Opts<'_>) -> Result<Vec<Vec<u8>>, FlowError> {
    let refused =
        || rejected("ERR flow limit reservation_ids must contain one unique id per credit");
    let Some(Term::List(items)) = opts.get("reservation_ids") else {
        return Err(refused());
    };
    if items.is_empty() || items.len() > usize::try_from(MAX_AMOUNT).unwrap_or(0) {
        return Err(refused());
    }
    let mut ids: Vec<Vec<u8>> = Vec::new();
    for item in items {
        match item {
            Term::Bin(id)
                if !id.is_empty() && id.len() <= MAX_RESERVATION_ID_BYTES && !ids.contains(id) =>
            {
                ids.push(id.clone());
            }
            _ => return Err(refused()),
        }
    }
    match opts.get("amount") {
        None => Ok(ids),
        Some(Term::Int(amount))
            if usize::try_from(*amount).is_ok_and(|amount| amount == ids.len()) =>
        {
            Ok(ids)
        }
        Some(_) => Err(refused()),
    }
}

fn fields(op: Op, opts: &Opts<'_>) -> Result<Fields, FlowError> {
    match op {
        Op::Lease => {
            let amount = amount(opts)?;
            let ttl_ms = ttl(opts, true)?;
            let now_ms = now_ms(opts)?;
            deadline(now_ms, ttl_ms)?;
            let configuration = configuration(opts)?;
            Ok(Fields {
                amount,
                ttl_ms,
                now_ms,
                ids: Vec::new(),
                configuration,
            })
        }
        Op::Spend => {
            let amount = amount(opts)?;
            let now_ms = now_ms(opts)?;
            let ttl_ms = ttl(opts, false)?;
            deadline(now_ms, ttl_ms)?;
            Ok(Fields {
                amount,
                ttl_ms,
                now_ms,
                ..Fields::default()
            })
        }
        Op::Release => {
            let ids = release_ids(opts)?;
            Ok(Fields {
                amount: u64::try_from(ids.len()).unwrap_or(0),
                now_ms: now_ms(opts)?,
                ids,
                ..Fields::default()
            })
        }
        Op::Get => Ok(Fields {
            now_ms: now_ms(opts)?,
            ..Fields::default()
        }),
    }
}

pub(super) fn scope_key(scope: &[u8]) -> Result<(), FlowError> {
    let prefix = keys::governance_prefix(&Partition::parse(scope), "limit").len();
    key_size(prefix.saturating_add(scope.len()))
}

/// Reads `FLOW.LIMIT.LEASE`, `SPEND`, `RELEASE` or `GET`.
///
/// # Errors
/// Elixir's text for a refused scope or option, or an unknown command.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    let (op, allowed): (Op, &[&str]) = match name {
        "FLOW.LIMIT.LEASE" => (
            Op::Lease,
            &[
                "scope",
                "shard_id",
                "amount",
                "ttl_ms",
                "now_ms",
                "limit",
                "config_version",
                "policy_version",
                "deadline_ms",
            ],
        ),
        "FLOW.LIMIT.SPEND" => (
            Op::Spend,
            &[
                "scope",
                "shard_id",
                "amount",
                "now_ms",
                "ttl_ms",
                "reservation_ids",
                "limit",
                "config_version",
                "policy_version",
                "deadline_ms",
            ],
        ),
        "FLOW.LIMIT.RELEASE" => (
            Op::Release,
            &[
                "scope",
                "shard_id",
                "reservation_ids",
                "amount",
                "now_ms",
                "deadline_ms",
            ],
        ),
        "FLOW.LIMIT.GET" => (Op::Get, &["scope", "now_ms", "deadline_ms"]),
        _ => {
            return Err(FlowError::rejected(format!(
                "ERR {name} is not supported by this server"
            )));
        }
    };
    let scope = native(args, "scope")?;
    let opts = Opts::new(name, args, allowed)?;
    if scope.is_empty() {
        return Err(rejected("ERR flow limit scope must be a non-empty string"));
    }
    scope_key(&scope)?;
    let late = if op == Op::Spend {
        if opts.has("reservation_ids") {
            Err(rejected(
                "ERR flow limit reservation_ids cannot be supplied for spend",
            ))
        } else {
            configuration(&opts)
        }
    } else {
        Ok(Configuration::default())
    };
    Ok(Command {
        op,
        shard: (op != Op::Get).then(|| opts.get("shard_id").cloned().unwrap_or(Term::Nil)),
        shard_id: 0,
        fields: fields(op, &opts),
        late,
        scope,
    })
}
