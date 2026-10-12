//! One bounded physical-state claim phase at ordered apply, after request replay.
//!
//! This leaf phase does not implement automatic reclaim ratios, timeout/governance
//! orchestration or durable cursors. Complete coherent indexes and atomic durable
//! persistence remain the outer adapter's duties.

use std::collections::BTreeMap;

use wf_types::identity::PartitionId;

use crate::workflow::admission::AdmissionError;
use crate::workflow::indexed::IndexedGroupView;
use crate::workflow::request::gate::{CommittedHeader, GateView};

use super::{PartitionFilter, ScopedRecordId};

mod admission;
pub(crate) mod decision;
mod result;
mod token;

pub use result::{ClaimDecision, ClaimPlan, ClaimRejection, ClaimResume};

/// Physical lifecycle state inclusion; running is independent of its logical lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimStates<'a> {
    /// Every eligible physical state.
    Any,
    /// A nonempty unique list of exact physical states.
    Only(&'a [&'a [u8]]),
}

/// Borrowed semantic query; generated tokens and effective time are excluded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimQuery<'a> {
    /// Exact workflow type.
    pub workflow_type: &'a [u8],
    /// Authoritative partition filter.
    pub partitions: PartitionFilter<'a>,
    /// Included physical lifecycle states.
    pub states: ClaimStates<'a>,
    /// Excluded physical states, applied after inclusion.
    pub exclude_states: &'a [&'a [u8]],
    /// Optional source priority band zero through two.
    pub priority: Option<u8>,
    /// Nonempty opaque binary worker identity.
    pub worker: &'a [u8],
    /// Positive lease duration in milliseconds.
    pub lease_ms: u64,
    /// Maximum selected records in this phase.
    pub limit: usize,
}

/// Actual encoding owners verified by the outer committed decoder/authority reader.
#[derive(Clone, Copy, Debug)]
pub struct ClaimEncoding<'a> {
    /// Canonical typed partition whose placement is checked again at apply.
    pub partition: &'a PartitionId,
    /// Actual state key bound to this partition and ID; lengths are checked here.
    pub state_key: &'a [u8],
}

/// Coherent retained, canonical-record, policy and complete index authority.
#[derive(Clone, Copy, Debug)]
pub struct ClaimApplyView<'a> {
    /// Original request namespace, retained outcomes and processing control.
    pub gate: GateView<'a>,
    /// Canonical relevant records, modes, placement and complete correctness indexes.
    pub group: IndexedGroupView<'a>,
    /// Snapshot identity; equal revision means identical contents in this namespace.
    pub revision: u64,
    /// Actual immutable state keys and typed partitions for consumed identities.
    pub encodings: &'a BTreeMap<ScopedRecordId<'a>, ClaimEncoding<'a>>,
}

/// Explicit committed inputs; no leader-selected record or resource authority.
#[derive(Clone, Copy, Debug)]
pub struct ClaimEnvelope<'a> {
    /// Trusted stamped original request header.
    pub header: CommittedHeader<'a>,
    /// Normalized physical-state query.
    pub query: ClaimQuery<'a>,
    /// Source token algorithm version; version one preserves binary decimal tokens.
    pub token_version: u16,
    /// Next available state-entry sequence ordinal; this phase consumes none.
    pub first_ordinal: u32,
    /// Bounded merged candidate scans.
    pub scan_budget: usize,
    /// Independent bounded lane-head reads, including empty lanes.
    pub head_budget: usize,
    /// Opaque zero-write cursor; mutation requires restarting with a fresh request ID.
    pub resume: Option<&'a ClaimResume<'a>>,
}

/// Trusted immutable represented-work profile, separate from prepared query hints.
///
/// This deterministic logical-byte estimate is not a CPU, allocator or RSS bound.
/// Required source workloads need sufficiently large replicated profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimWorkLimits {
    max_work_bytes: u64,
}
impl Default for ClaimWorkLimits {
    fn default() -> Self {
        Self {
            max_work_bytes: 128 * 1_024 * 1_024,
        }
    }
}
impl ClaimWorkLimits {
    /// Selects a positive replicated work profile; an entry cannot raise this limit.
    ///
    /// # Errors
    /// Rejects zero with the existing immutable-profile construction error.
    pub const fn new(max_work_bytes: u64) -> Result<Self, AdmissionError> {
        if max_work_bytes == 0 {
            Err(AdmissionError::InvalidLimits)
        } else {
            Ok(Self { max_work_bytes })
        }
    }
    /// Reads the separately supplied effective apply profile.
    #[must_use]
    pub const fn max_work_bytes(self) -> u64 {
        self.max_work_bytes
    }
}
