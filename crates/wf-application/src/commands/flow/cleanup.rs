//! `FLOW.RETENTION_CLEANUP` (ADR-014 step 9), as
//! `Ferricstore.Flow.Retention.cleanup/2` reads and answers it: fail the
//! Flows whose active time ran out, then remove terminal Flows whose
//! retention ended, up to `limit` of both together across the shards; an
//! opaque continuation resumes where the limit stopped.

use super::FlowError;
use super::opts::Opts;
use super::term::{Term, map};

/// Elixir's default `limit`.
const DEFAULT_LIMIT: u64 = 100;
/// The first byte of this server's continuation tokens.
const TOKEN: u8 = 1;

/// One cleanup request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionCleanup {
    /// Most Flows timed out and removed together.
    pub limit: u64,
    /// Command time; the wall clock when absent.
    pub now_ms: Option<u64>,
    /// The shard a continuation resumes at.
    pub from_shard: u16,
}

/// What one cleanup did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cleaned {
    /// Terminal Flows removed.
    pub flows: u64,
    /// History events removed with them.
    pub history: u64,
    /// Stored values removed with them.
    pub values: u64,
    /// Flows failed for running past `max_active_ms`.
    pub active_timeouts: u64,
}

impl Cleaned {
    /// Flows this counts against the limit.
    #[must_use]
    pub const fn taken(&self) -> u64 {
        self.flows.saturating_add(self.active_timeouts)
    }

    /// Both counts together.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            flows: self.flows.saturating_add(other.flows),
            history: self.history.saturating_add(other.history),
            values: self.values.saturating_add(other.values),
            active_timeouts: self.active_timeouts.saturating_add(other.active_timeouts),
        }
    }
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// Reads `FLOW.RETENTION_CLEANUP`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse(args: &Term) -> Result<RetentionCleanup, FlowError> {
    let opts = Opts::new(
        "FLOW.RETENTION_CLEANUP",
        args,
        &["limit", "now_ms", "continuation"],
    )?;
    let limit = match opts.get("limit") {
        None => DEFAULT_LIMIT,
        Some(Term::Int(limit)) if *limit > 0 => u64::try_from(*limit).unwrap_or(DEFAULT_LIMIT),
        Some(_) => return Err(rejected("ERR flow limit must be a positive integer")),
    };
    let now_ms = match opts.get("now_ms") {
        None => None,
        Some(Term::Int(now)) if *now >= 0 => u64::try_from(*now).ok(),
        Some(_) => return Err(rejected("ERR flow now_ms must be a non-negative integer")),
    };
    let from_shard = match opts.get("continuation") {
        None | Some(Term::Nil) => 0,
        Some(Term::Bin(token)) => match token.as_slice() {
            [TOKEN, high, low] => u16::from_be_bytes([*high, *low]),
            _ => {
                return Err(rejected(
                    "ERR flow continuation is not valid for this server",
                ));
            }
        },
        Some(_) => return Err(rejected("ERR flow continuation must be a binary token")),
    };
    Ok(RetentionCleanup {
        limit,
        now_ms,
        from_shard,
    })
}

/// The reply: the counts, and a continuation when the limit stopped the
/// cleanup before shard `next`.
#[must_use]
pub fn reply(cleaned: Cleaned, next: Option<u16>) -> Term {
    let continuation = next.map_or(Term::Nil, |shard| {
        let [high, low] = shard.to_be_bytes();
        Term::Bin(vec![TOKEN, high, low])
    });
    map(vec![
        ("active_timeouts", Term::int(cleaned.active_timeouts)),
        ("continuation", continuation),
        ("flows", Term::int(cleaned.flows)),
        ("history", Term::int(cleaned.history)),
        ("more?", Term::Bool(next.is_some())),
        ("values", Term::int(cleaned.values)),
    ])
}
