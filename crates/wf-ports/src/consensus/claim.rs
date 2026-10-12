//! Owned neutral inputs for one physical claim phase, without apply authority.

/// Explicit query values; fresh domain admission follows original request replay.
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedClaimQuery {
    /// Exact opaque workflow type, including structurally representable invalid input.
    pub workflow_type: Vec<u8>,
    /// None means every physical state; Some preserves the exact list and its order.
    pub states: Option<Vec<Vec<u8>>>,
    /// Exact physical states excluded after inclusion.
    pub exclude_states: Vec<Vec<u8>>,
    /// None means every partition; Some preserves exact opaque components.
    pub partitions: Option<Vec<Vec<u8>>>,
    /// Optional priority; fresh domain validates the source zero-through-two range.
    pub priority: Option<u8>,
    /// Opaque worker bytes; fresh domain requires a nonempty identity.
    pub worker: Vec<u8>,
    /// Requested lease duration; fresh domain validates duration and deadline.
    pub lease_ms: u64,
    /// Selected-record hint; the private format uses a checked u32 representation.
    pub limit: usize,
}

/// One prepared leaf phase; selection consults coherent authoritative apply indexes.
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedClaimPhase {
    /// Explicit query with no selected-record or current-policy authority.
    pub query: PreparedClaimQuery,
    /// Explicit source token algorithm version, checked by fresh apply.
    pub token_version: u16,
    /// Next available sequence ordinal; this leaf consumes no creation ordinal.
    pub first_ordinal: u32,
    /// Candidate scan hint, encoded as checked u32 and admitted by fresh apply.
    pub scan_budget: usize,
    /// Independent lane-head hint, encoded as checked u32 and admitted by fresh apply.
    pub head_budget: usize,
}
