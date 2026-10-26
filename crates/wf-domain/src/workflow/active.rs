//! Fresh atomic max-active closure over one coherent group and captured lifecycle.
//!
//! The outer apply owner binds namespace, recovery, ordered log and release shard.
//! This pure predecessor provides no durable adapter or public claim orchestration.

use super::admission::{AdmissionError, AdmissionLimits};
use super::batch::ApplyError;
use super::indexed::{EntryOrder, IndexedGroupView};
use crate::claim::ScopedRecordId;
use crate::value::DomainError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use wf_types::version::TimestampMillis;

mod admission;
mod authority;
mod capture;
mod history;
mod indexes;
mod plan;
mod record;
mod releases;
mod trace;
mod values;

pub(crate) use capture::capture_lifecycle_layers;
pub use capture::{
    ActiveValueError, CapturedLifecycle, LifecycleDefaults, LifecycleOverrides, MaxActiveOverride,
    capture_lifecycle, restore_captured_lifecycle,
};
pub use history::{ActiveHistory, HistoryKey, HistoryTrim};
pub use indexes::{ActiveIndexDelta, ActiveIndexKey, ActiveIndexRows, active_rows, index_deltas};
pub use plan::{ActiveRecordWrite, ActiveTimeoutPlan};
pub use record::{ActiveCarry, ActiveRecord, ActiveRecordData, restore_active};
pub use releases::{
    GovernanceReservation, LimitEnforcement, ReleaseAppend, ReleaseIntent, ReleaseOutboxState,
};
pub(crate) use values::value_ref_bytes;
pub use values::{
    ActiveValueKind, ActiveValueRef, ActiveValueRefs, TimeoutError, TimeoutValueWrite,
};

/// Complete original authority; all maps must derive from the same canonical view.
#[derive(Clone, Copy)]
pub struct ActiveTimeoutView<'v> {
    /// Group/core/ordering/mode projections with complete lane and due authority.
    pub indexed: IndexedGroupView<'v>,
    /// Complete relevant rich records; absence is an outer authority obligation.
    pub records: &'v BTreeMap<ScopedRecordId<'v>, &'v ActiveRecord>,
    /// Complete relevant ordered history key sets.
    pub history: &'v BTreeMap<ScopedRecordId<'v>, BTreeSet<HistoryKey<'v>>>,
    /// Enabled `DueAny` and every applicable semantic index row.
    pub lifecycle: &'v BTreeMap<ActiveIndexKey<'v>, u64>,
    /// Complete local release outbox scalars.
    pub outbox: &'v ReleaseOutboxState,
    /// Trusted selected release shard; namespace/log binding stays outer.
    pub release_shard: u64,
}

/// Trusted immutable represented closure work profile, never an entry field.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TimeoutLimits {
    visits: u64,
    steps: u64,
    links: u64,
    history_deletes: u64,
    work_bytes: u64,
}
impl Default for TimeoutLimits {
    fn default() -> Self {
        Self {
            visits: 100_000,
            steps: 100_000,
            links: 100_000,
            history_deletes: 100_000,
            work_bytes: 128 * 1_024 * 1_024,
        }
    }
}
impl TimeoutLimits {
    /// Selects positive counts/work; this is a new profile rather than a source ceiling.
    ///
    /// # Errors
    /// Rejects zero. Required source workloads must select sufficient trusted limits.
    pub const fn new(
        visits: u64,
        steps: u64,
        links: u64,
        history_deletes: u64,
        work_bytes: u64,
    ) -> Result<Self, AdmissionError> {
        if visits == 0 || steps == 0 || links == 0 || history_deletes == 0 || work_bytes == 0 {
            Err(AdmissionError::InvalidLimits)
        } else {
            Ok(Self {
                visits,
                steps,
                links,
                history_deletes,
                work_bytes,
            })
        }
    }
    /// Maximum cumulative represented visits.
    #[must_use]
    pub const fn max_visits(self) -> u64 {
        self.visits
    }
    /// Maximum mutation steps.
    #[must_use]
    pub const fn max_steps(self) -> u64 {
        self.steps
    }
    /// Maximum traversed lineage links.
    #[must_use]
    pub const fn max_links(self) -> u64 {
        self.links
    }
    /// Maximum consumed history-deletion prefix.
    #[must_use]
    pub const fn max_history_deletes(self) -> u64 {
        self.history_deletes
    }
    /// Maximum cumulative P+G represented bytes.
    #[must_use]
    pub const fn max_work_bytes(self) -> u64 {
        self.work_bytes
    }
}

/// Deterministic request/profile rejection; operational corruption is separate.
#[derive(Debug, Eq, PartialEq)]
pub enum ActiveTimeoutRejection {
    /// Existing actual byte/count admission error.
    Admission(AdmissionError),
    /// Existing core numeric/counter business error.
    Domain(DomainError),
    /// Requested root belongs to another group.
    CrossGroup,
    /// Cumulative represented work exceeds the trusted profile.
    WorkBytes {
        /// Prospective actual represented charge.
        bytes: u64,
        /// Trusted positive maximum.
        maximum: u64,
    },
    /// Visit count exceeded.
    Visits {
        /// Prospective count.
        count: u64,
        /// Trusted maximum.
        maximum: u64,
    },
    /// Mutation step count exceeded.
    Steps {
        /// Prospective count.
        count: u64,
        /// Trusted maximum.
        maximum: u64,
    },
    /// Lineage link count exceeded.
    Links {
        /// Prospective count.
        count: u64,
        /// Trusted maximum.
        maximum: u64,
    },
    /// History-deletion prefix count exceeded.
    HistoryDeletes {
        /// Prospective count.
        count: u64,
        /// Trusted maximum.
        maximum: u64,
    },
    /// Valid release outbox backlog cannot accept all qualifying occurrences.
    ReleaseBacklog,
}
impl fmt::Display for ActiveTimeoutRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission(e) => e.fmt(f),
            Self::Domain(e) => e.fmt(f),
            _ => f.write_str(match self {
                Self::CrossGroup => "CROSSSLOT timeout roots belong to different groups",
                Self::WorkBytes { .. } => "workflow timeout work byte budget exceeded",
                Self::Visits { .. } => "workflow timeout visit budget exceeded",
                Self::Steps { .. } => "workflow timeout step budget exceeded",
                Self::Links { .. } => "workflow timeout link budget exceeded",
                Self::HistoryDeletes { .. } => "workflow timeout history deletion budget exceeded",
                Self::ReleaseBacklog => "ERR flow governance release outbox backlog is full",
                Self::Admission(_) | Self::Domain(_) => "invalid workflow timeout",
            }),
        }
    }
}
impl std::error::Error for ActiveTimeoutRejection {}

/// Atomic fresh result; a rejection exposes no partial output plan.
/// Accepted results move the admitted fixed metadata into one ordinary host Box,
/// including empty plans; leaf owners are never cloned by that allocation.
pub enum ActiveTimeoutDecision<'v> {
    /// All same-group effects passed authority/resource checks before owner copies.
    Accepted(Box<ActiveTimeoutPlan<'v>>),
    /// Deterministic request/profile failure; original authority stays unchanged.
    Rejected(ActiveTimeoutRejection),
}

/// Expires bounded explicit scoped roots and their complete same-group closure.
///
/// Roots and their object lifetime are never retained. Current lifecycle comes
/// from captured records. Caller order, depth-first cancellation and byte-ordered
/// group/member enumeration are explicit deterministic choices. All effects need
/// one later atomic persistence operation; there is no control/outcome write here.
///
/// # Errors
/// Rejects corrupt/incomplete authority, active-path cycles and unsupported index
/// profiles operationally. Host failures cannot become durable business rejection.
pub fn expire_active_batch<'v>(
    view: ActiveTimeoutView<'v>,
    roots: &[ScopedRecordId<'_>],
    now: TimestampMillis,
    order: EntryOrder,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<ActiveTimeoutDecision<'v>, ApplyError> {
    let result = trace::discover_trace(view, roots, now, order, limits, work)
        .and_then(|trace| plan::materialize_trace(trace, limits, work));
    match result {
        Ok(plan) => Ok(ActiveTimeoutDecision::Accepted(Box::new(plan))),
        Err(Ok(error)) => Ok(ActiveTimeoutDecision::Rejected(error)),
        Err(Err(error)) => Err(error),
    }
}
