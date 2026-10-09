//! Single-record claim inputs and pure selection from authoritative indexes.

use crate::lease::LeaseToken;

mod error;
mod index;
mod request;
mod result;
mod selection;

pub use error::SelectionError;
pub use index::{
    AuthoritativeClaimView, ClaimRecord, DueOrder, FifoIndex, LaneKey, LaneOrder, ParallelIndex,
    RecordIndex, ScopedRecordId, decode_member, encode_member,
};
pub use request::{PartitionFilter, SelectionRequest, StateFilter};
pub use result::{
    ClaimSelection, SelectedClaim, SelectionAction, SelectionContinuation, SelectionStop,
};
pub use selection::select;

/// A claim of a named record, checked again against authoritative state.
///
/// Claim eligibility here covers one record's state and due time only. The
/// caller must separately establish scope, FIFO and policy eligibility before
/// using this operation in any future partition-selection implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claim {
    /// Exact lifecycle state expected by the caller; `running` requires reclaim.
    pub expected_state: Vec<u8>,
    /// Nonempty binary worker identity.
    pub worker: Vec<u8>,
    /// Newly prepared opaque lease token, committed with this command.
    pub token: LeaseToken,
    /// Positive lease duration in milliseconds.
    pub lease_ms: u64,
}
