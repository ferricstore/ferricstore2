//! Explicit configured scopes, bounded observations and nonauthoritative stats.
use std::fmt;
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    log::CodecLimits,
    state::{CreditCatalogReadLimits, StateTransactionError},
};

/// Configured source; member bytes cannot create this routing capability.
#[derive(Clone, Copy)]
pub struct CreditCleanupSource<'a> {
    /// Verified source namespace.
    pub scope: &'a RequestScope,
    /// Actual configured owner shard count.
    pub shard_count: u64,
}
/// Borrowed configuration and explicit tick time; no cleanup authority.
#[derive(Clone, Copy)]
pub struct CreditCleanupRequest<'a> {
    /// Verified progress/catalog namespace.
    pub target: &'a RequestScope,
    /// Admitted source namespaces in configuration order.
    pub sources: &'a [CreditCleanupSource<'a>],
    /// Exact catalog identity.
    pub catalog: &'a [u8],
    /// Due hint and lower bound for trusted committed command preparation.
    pub now_ms: u64,
}
/// Positive read, decode, staging and supervised execution profiles.
#[derive(Clone, Copy, Debug)]
pub struct CreditCleanupLimits {
    /// Complete private request/response profile.
    pub codec: CodecLimits,
    /// Each coherent owner/catalog/progress guard's limits.
    pub read: CreditCatalogReadLimits,
    /// Bounded configured-source search work.
    pub maximum_sources: usize,
    /// Supervised execution owner ceiling.
    pub execution_workspace_bytes: usize,
    /// Enclosing simultaneous logical owner ceiling.
    pub workspace_bytes: usize,
}
impl Default for CreditCleanupLimits {
    fn default() -> Self {
        Self {
            codec: CodecLimits::default(),
            read: CreditCatalogReadLimits::default(),
            maximum_sources: 65536,
            execution_workspace_bytes: 256 * 1_048_576,
            workspace_bytes: 1024 * 1_048_576,
        }
    }
}
/// Successful original cleanup/prune result with confirmed effective cursor.
#[derive(Debug, Eq, PartialEq)]
pub struct CreditCleanupReply {
    /// Actual confirmed deleted reservation count, at most256.
    pub deleted: u16,
    /// Another cleanup page is pending for this owner.
    pub pending: bool,
    /// A missing owner member was actually unregistered.
    pub pruned: bool,
    /// Last observed or confirmed persisted traversal hint.
    pub next_cursor: Option<Vec<u8>>,
}
/// Fixed per-step failure; no partial accepted reply is manufactured.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCleanupFailure {
    /// A bounded source/target observation failed.
    Read(StateTransactionError),
    /// Member/page/scope did not match its configured contract.
    InvalidObservation,
    /// Cleanup was refused, unknown or had an invalid original retained reply.
    CleanupUnconfirmed,
    /// Missing-owner membership removal was not confirmed.
    PruneUnconfirmed,
    /// Progress reset/write was not confirmed.
    ProgressUnconfirmed,
}
impl fmt::Display for CreditCleanupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(f),
            Self::InvalidObservation => f.write_str("invalid cleanup observation"),
            Self::CleanupUnconfirmed => f.write_str("cleanup unconfirmed"),
            Self::PruneUnconfirmed => f.write_str("catalog prune unconfirmed"),
            Self::ProgressUnconfirmed => f.write_str("cleanup progress unconfirmed"),
        }
    }
}
impl std::error::Error for CreditCleanupFailure {}
/// Original step accounting, including failed attempted commands.
#[derive(Debug, Eq, PartialEq)]
pub struct CreditCleanupStep {
    /// Complete successful reply or fixed failure.
    pub reply: Result<CreditCleanupReply, CreditCleanupFailure>,
    /// Attempted cleanup/prune/progress commands.
    pub commands: u16,
    /// Confirmed deletions, preserved even if progress later fails.
    pub deleted: u16,
    /// Per-step failure count.
    pub errors: u16,
    /// Traversal wrapped only through confirmed effective progress.
    pub wrapped: bool,
    pub(super) streak: Option<Vec<u8>>,
    pub(super) streak_pages: u8,
}
/// Bounded tick accounting; no cache, clock or authority certificate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CreditCleanupStats {
    /// Every attempted mutation shares this count.
    pub commands: u16,
    /// Sum of confirmed deleted reservations.
    pub deleted: u32,
    /// Sum of bounded step failures.
    pub errors: u16,
    /// A confirmed wrapped cycle deleted nothing.
    pub caught_up: bool,
    /// At least one step actually wrapped.
    pub wrapped: bool,
}
/// Whole-call refusal before observations or effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCleanupError {
    /// Invalid namespace, time, source, budget or required positive profile.
    InvalidOptions,
    /// Prospective enclosing ownership overflow/limit.
    Limit,
}
impl fmt::Display for CreditCleanupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidOptions => "invalid credit cleanup options",
            Self::Limit => "credit cleanup workspace limit",
        })
    }
}
impl std::error::Error for CreditCleanupError {}
