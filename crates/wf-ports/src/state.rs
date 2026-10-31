//! State boundary for `wf-ports`.
//!
//! Source compatibility policy queries use an explicit policy read capability.

mod claim;
mod control;
mod core_policy;
mod credit_catalog;
mod governance_catalog;
mod history;
mod original_policy;
mod policy;
mod transaction;
mod workflow_release;
pub use workflow_release::{WorkflowReleaseEntry, WorkflowReleasePage};

pub use claim::{
    ClaimDueKind, ClaimDueStep, ClaimLaneStep, GroupClaimChanges, GroupClaimDueCursor,
    GroupClaimLaneCursor, GroupClaimTransaction,
};
pub use control::{GroupControlChanges, GroupControlTransaction};
pub use core_policy::{GroupCorePolicyChanges, GroupCorePolicyTransaction};
pub use credit_catalog::{
    CreditCatalogEntry, CreditCatalogMemberRef, CreditCatalogOwnerState, CreditCatalogPage,
    CreditCatalogReadLimits, CreditCatalogReadPort, CreditOwnerReadPort,
    EncodedCreditCatalogMembers, decode_credit_catalog_member, encode_credit_catalog_members,
};
pub use governance_catalog::{
    GovernanceCatalogPage, GovernanceCatalogProgress, GovernanceCatalogProgressPort,
    GovernanceCatalogPrunePage, GovernanceCatalogPruneReadPort, GovernanceCatalogRead,
    GovernanceCatalogReadLimits, GovernanceCatalogReadPort, LoadedGovernanceRecord,
    encode_catalog_member_revision, encode_catalog_progress_cursor,
    inspect_catalog_member_revision, inspect_catalog_progress_cursor,
};
pub use history::{
    StoredWorkflowHistoryRow, StoredWorkflowHistorySnapshot, WorkflowHistoryDirection,
    WorkflowHistoryHydrationError, WorkflowHistoryHydrationLimits, WorkflowHistoryHydrationRequest,
    WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits, WorkflowHistoryValuePage,
};
pub use original_policy::{GroupOriginalPolicyChanges, GroupOriginalPolicyTransaction};
pub use policy::{OriginalPolicyGetReader, StoredPolicyValue, read_original_policy_get_reply};
pub use transaction::{
    DueCursorStep, GroupApplyChanges, GroupApplyStore, GroupApplyTransaction, GroupDueCursor,
    StoredWorkflowRecord,
};

use std::fmt;

/// Operational transaction failure, never retained as a business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateTransactionError {
    /// A constructor profile is zero, overflowing or internally incompatible.
    InvalidLimits,
    /// A healthy existing inventory or manifest belongs to another namespace.
    NamespaceConflict,
    /// A healthy private backend layout requires an explicitly selected migration.
    UnsupportedLayout,
    /// Valid alternate backing needs the unfinished format interoperability route.
    UnsupportedPolicyFormat,
    /// A supported policy change needs unfinished authoritative indexed effects.
    UnsupportedPolicyEffects,
    /// Valid original backing needs another captured atom/profile context.
    UnsupportedPolicyProfile,
    /// A reached source map needs actual runtime order unavailable in this profile.
    UnsupportedSourceOrder,
    /// The adapter proves the durable predecessor is unchanged.
    KnownAborted,
    /// Commit may be durable; fence service and reconcile after reopen.
    Uncertain,
    /// Stored namespace, record or index authority is inconsistent.
    Corrupt,
    /// The backend cannot currently provide authority.
    Unavailable,
    /// A positive explicit cumulative read or local cursor cap was exhausted.
    Limit,
    /// An admitted host reservation failed.
    Allocation,
    /// The supplied mathematical change container is incoherent.
    InvalidChanges,
    /// The original continuation member disappeared between coherent reads.
    CatalogChanged,
}

impl fmt::Display for StateTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "invalid state store limits",
            Self::NamespaceConflict => "state store namespace conflict",
            Self::UnsupportedLayout => "unsupported state store layout",
            Self::UnsupportedPolicyFormat => "unsupported policy format interoperability",
            Self::UnsupportedPolicyEffects => "unsupported policy indexed effects",
            Self::UnsupportedPolicyProfile => "unsupported original policy profile",
            Self::UnsupportedSourceOrder => "unsupported original source order",
            Self::KnownAborted => "state transaction known aborted",
            Self::Uncertain => "state transaction commit uncertain",
            Self::Corrupt => "corrupt state authority",
            Self::Unavailable => "state authority unavailable",
            Self::Limit => "state transaction read limit",
            Self::Allocation => "state transaction allocation failed",
            Self::InvalidChanges => "invalid state transaction changes",
            Self::CatalogChanged => "ERR flow governance catalog changed during traversal",
        })
    }
}
impl std::error::Error for StateTransactionError {}
