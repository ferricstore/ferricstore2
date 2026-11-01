//! Native owners and borrowed-document coordinates, without publication grants.
use super::super::{OriginalQueryRecordError, OriginalQueryRecordLimits};
use crate::{blob::WorkflowValueBinding, state::StoredWorkflowRecord};
use std::{fmt, ops::Range};
use wf_domain::{
    metadata::record::{OriginalRecordMetadata, OriginalRecordMetadataError},
    policy::complete::{CompletePolicyEncodingError, PolicySchemaLimits},
    value::OriginalValueLimits,
    workflow::control::ControlState,
};

/// Whole-parent limits include already acquired read work and retained owners.
#[derive(Clone, Copy)]
pub struct NativeQueryPreparationLimits {
    /// Final ownership, whole-parent work and peak temporary ownership.
    pub query: OriginalQueryRecordLimits,
    /// Independent actual metadata grammar/byte/node/edge/depth/work ceilings.
    pub metadata: OriginalValueLimits,
    /// Actual selected policy grammar/work profile; whole-parent work also applies.
    pub policy: PolicySchemaLimits,
}

/// Typed native core and captured metadata, with actual policy index configuration.
pub struct NativeQueryRecord {
    pub(super) native: StoredWorkflowRecord,
    pub(super) values: Box<WorkflowValueBinding>,
    pub(super) metadata: OriginalRecordMetadata,
    pub(super) partition: Range<usize>,
    pub(super) scope: Option<Range<usize>>,
    pub(super) indexed: [Option<Vec<u8>>; 3],
    pub(super) state_index: Option<Vec<u8>>,
    pub(super) policy_profile: u8,
    pub(super) policy_generation: Option<u64>,
}
impl NativeQueryRecord {
    /// Actual native core/ordering, preserving the complete u128 sequence.
    #[must_use]
    pub const fn native_record(&self) -> &StoredWorkflowRecord {
        &self.native
    }
    /// Actual immutable lifecycle/value provenance; captured hints never override it.
    #[must_use]
    pub fn value_binding(&self) -> &WorkflowValueBinding {
        &self.values
    }
    /// Unchanged typed caller capture; this is not a server-derived postimage.
    #[must_use]
    pub const fn captured_metadata(&self) -> &OriginalRecordMetadata {
        &self.metadata
    }
    /// Checked access to the validated nonempty logical partition donor.
    #[must_use]
    pub fn logical_partition(&self) -> Option<&[u8]> {
        self.metadata.document().byte_range(self.partition.clone())
    }
    /// Explicit known scope, with None representing captured unscoped data.
    #[must_use]
    pub fn scope_prefix(&self) -> Option<&[u8]> {
        self.scope
            .clone()
            .and_then(|r| self.metadata.document().byte_range(r))
    }
    /// Normalized names from the actual current policy, without retaining its body.
    pub fn indexed_attributes(&self) -> impl Iterator<Item = &[u8]> {
        self.indexed.iter().flatten().map(Vec::as_slice)
    }
    /// Normalized state metadata name from the actual current policy.
    #[must_use]
    pub fn indexed_state_metadata(&self) -> Option<&[u8]> {
        self.state_index.as_deref()
    }
    /// Observed backing profile: zero absent, one Complete, two Original.
    #[must_use]
    pub const fn policy_profile(&self) -> u8 {
        self.policy_profile
    }
    /// Actual current generation; absent policy stays distinct from a birth witness.
    #[must_use]
    pub const fn policy_generation(&self) -> Option<u64> {
        self.policy_generation
    }
}

/// One prepared physical observation; visibility, authorization and barriers are external.
pub struct NativeQuerySnapshot {
    pub(super) control: ControlState,
    pub(super) record: Option<NativeQueryRecord>,
    pub(super) work: usize,
    pub(super) temporary: usize,
    pub(super) owned: usize,
}
impl NativeQuerySnapshot {
    /// Move the existing record owner without copying metadata, core or references.
    #[must_use]
    pub fn into_record(self) -> Option<NativeQueryRecord> {
        self.record
    }
    /// Control observed on the same native read revision.
    #[must_use]
    pub const fn control(&self) -> ControlState {
        self.control
    }
    /// The physical record, or actual absence under the supported native capability.
    #[must_use]
    pub const fn record(&self) -> Option<&NativeQueryRecord> {
        self.record.as_ref()
    }
    /// Final conservative logical ownership, excluding discarded policy backing.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned
    }
    /// Whole-parent read/decode/inspection/preparation work.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Whole-parent peak logical ownership, distinct from allocator/RSS.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}
impl fmt::Debug for NativeQuerySnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeQuerySnapshot(<redacted>)")
    }
}

/// Operational/data preparation refusal; no store mutation or partial query result.
#[derive(Debug)]
pub enum NativeQueryPreparationError {
    /// A required ceiling is zero.
    InvalidLimits,
    /// Whole-parent ownership/work/temporary limit or checked arithmetic overflow.
    Limit,
    /// Record and current-policy observations disagree.
    InvalidCapture,
    /// Logical partition or scope was not captured, and cannot be inferred.
    UnknownRouting,
    /// Captured logical scope/partition does not match the native physical point.
    RoutingMismatch,
    /// Original field classes or query metadata are invalid.
    Query(OriginalQueryRecordError),
    /// Actual stored metadata/context failure.
    Metadata(OriginalRecordMetadataError),
    /// Actual backing/configuration/schema failure.
    Policy(CompletePolicyEncodingError),
}
impl fmt::Display for NativeQueryPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for NativeQueryPreparationError {}
