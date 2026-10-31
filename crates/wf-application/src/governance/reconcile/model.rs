//! Bounded configuration and original unresolved call ownership.
use super::super::{PreparedPublicCreditCall, PublicCreditCallLimits};
use std::fmt;
use wf_domain::{governance::release::ReleaseCursorError, workflow::request::RequestScope};
use wf_ports::state::GovernanceCatalogReadLimits;
/// Explicit configured source order and source-compatible invocation fields.
#[derive(Clone, Copy)]
pub struct ReleaseReconcileRequest<'a> {
    /// Verified outbox namespaces in configured shard order.
    pub sources: &'a [RequestScope],
    /// Single original v1 cursor, absent for a new scan.
    pub cursor: Option<&'a [u8]>,
    /// At most256 original intents from one nonempty source.
    pub reservation_limit: u16,
    /// Explicit finite lower bound for trusted mutation time.
    pub now_ms: u64,
}
/// Whole-call bounds, independently rechecked by each configured effect adapter.
#[derive(Clone, Copy, Debug)]
pub struct ReleaseReconcileLimits {
    /// Coherent page, record-fact and route owner limits.
    pub read: GovernanceCatalogReadLimits,
    /// Existing public credit caller contract.
    pub credit: PublicCreditCallLimits,
    /// Maximum configured source traversal count.
    pub maximum_sources: usize,
    /// Each original/decoded/encoded cursor owner ceiling.
    pub maximum_cursor_bytes: usize,
    /// Complete maintenance frame ceiling including codec staging.
    pub maximum_maintenance_bytes: usize,
    /// Supervised native maintenance execution owner ceiling.
    pub execution_workspace_bytes: usize,
    /// Whole simultaneous logical ownership ceiling.
    pub workspace_bytes: usize,
}
impl Default for ReleaseReconcileLimits {
    fn default() -> Self {
        Self {
            read: GovernanceCatalogReadLimits::default(),
            credit: PublicCreditCallLimits::default(),
            maximum_sources: 65536,
            maximum_cursor_bytes: 8 * 1_048_576,
            maximum_maintenance_bytes: 32 * 1_048_576,
            execution_workspace_bytes: 256 * 1_048_576,
            workspace_bytes: 4 * 1024 * 1_048_576,
        }
    }
}
/// Original page accounting and unresolved submitted credit contexts.
#[derive(Default)]
pub struct ReleaseReconcileStats {
    /// Confirmed released original intents, counting duplicate replay intents.
    pub released: u16,
    /// Physical running records still holding their exact original ID.
    pub retained: u16,
    /// Failed shard scans, per-intent failures and unconfirmed progress.
    pub errors: u64,
    /// Actual partition-group read attempts, excluding completed markers.
    pub read_batches: u16,
    /// Source v1 next scan hint; absent after a completely empty healthy cycle.
    pub next_cursor: Option<Vec<u8>>,
    /// Exact original public-credit calls with unknown outcomes. Resolve/retry
    /// these through the existing retry API; never regenerate their IDs/frames.
    pub pending: Vec<Box<PreparedPublicCreditCall>>,
}
/// Before-work configuration/resource refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseReconcileError {
    /// Invalid configured namespace, bounds or finite invocation fields.
    InvalidOptions,
    /// Prospective logical ownership overflow or limit.
    Limit,
    /// A bounded host reservation failed.
    Allocation,
    /// Complete source cursor validation refused.
    Cursor(ReleaseCursorError),
}
impl fmt::Display for ReleaseReconcileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => f.write_str("invalid release reconciliation options"),
            Self::Limit => f.write_str("release reconciliation resource limit"),
            Self::Allocation => f.write_str("release reconciliation allocation failed"),
            Self::Cursor(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ReleaseReconcileError {}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ReleaseStatus {
    Error,
    Retained,
    Completed,
    Release,
    Released,
}
