//! Captured child-group values; historical membership is not a backlink proof.

use crate::workflow::active::ActiveValueRef;
use std::collections::BTreeMap;
use wf_types::identity::{PartitionId, RecordId};

mod group;
pub(crate) use group::{
    count_terminal, mark_cancelled, resolve_group, running_children, validate_group,
};

/// Captured child waiting algebra.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ChildWait {
    /// Normal creation closes this group immediately with success.
    None,
    /// Wait for every child to be terminal.
    All,
    /// The first successful child may resolve success.
    Any,
}
/// Captured child failure behavior.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ChildFailure {
    /// Retain ordinary wait algebra after a failure.
    Ignore,
    /// Fail immediately on failure or cancellation.
    FailParent,
}
/// Captured action when the parent or a cancel group closes.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ParentClosed {
    /// Leave children independently active.
    Abandon,
    /// Cancel still-running captured members.
    Cancel,
}
/// Captured status of one child within one group.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ChildStatus {
    /// This group has not accounted for a terminal notification.
    Running,
    /// Successful child.
    Completed,
    /// Failed child.
    Failed,
    /// Cancelled captured member, including missing/previously terminal children.
    Cancelled,
}
/// Captured group resolution, independent of today's child states.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ChildResolution {
    /// Select the success target.
    Success,
    /// Select the failure target.
    Failure,
}
/// Actual child backlink; empty ID preserves the source no-parent branch.
#[derive(Clone, Eq, PartialEq)]
pub struct ParentLink {
    /// Opaque parent ID; absence and empty differ.
    pub id: Vec<u8>,
    /// Absent partition falls back to the child's own partition.
    pub partition: Option<PartitionId>,
}
/// Recorded terminal result references; payload hydration stays outer.
#[derive(Clone, Eq, PartialEq)]
pub struct ChildResult {
    /// Accounted terminal status.
    pub status: ChildStatus,
    /// Actual successful value reference, if present.
    pub result_ref: Option<ActiveValueRef>,
    /// Actual error reference, if present.
    pub error_ref: Option<ActiveValueRef>,
}
/// A group's own historical scoped member; no global uniqueness is imposed.
#[derive(Clone, Eq, PartialEq)]
pub struct ChildMember {
    /// Absent partition falls back to that group's parent partition.
    pub partition: Option<PartitionId>,
    /// Current accounted group status.
    pub status: ChildStatus,
    /// Terminal result, absent while running.
    pub result: Option<ChildResult>,
}
/// Exact group summary counters.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ChildSummary {
    /// Number of captured members.
    pub total: u64,
    /// Completed count.
    pub completed: u64,
    /// Failed count.
    pub failed: u64,
    /// Cancelled count.
    pub cancelled: u64,
}
/// Immutable captured policy and mutable accounting for one historical group.
#[derive(Clone, Eq, PartialEq)]
pub struct ChildGroup {
    /// Captured wait algebra.
    pub wait: ChildWait,
    /// Captured failure action.
    pub on_child_failed: ChildFailure,
    /// Captured parent-close action.
    pub on_parent_closed: ParentClosed,
    /// Actual nonempty success target, including arbitrary physical labels.
    pub success_state: Vec<u8>,
    /// Actual nonempty failure target.
    pub failure_state: Vec<u8>,
    /// Actual captured opaque idempotency bytes; no domain hashing occurs.
    pub request_hash: Vec<u8>,
    /// Byte-ordered unique members within this group only.
    pub children: BTreeMap<RecordId, ChildMember>,
    /// Exact summary consistent with the group's accounted statuses.
    pub summary: ChildSummary,
    /// Closed resolution; success does not imply every child is terminal.
    pub resolved: Option<ChildResolution>,
}
