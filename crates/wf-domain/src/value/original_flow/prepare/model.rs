//! Borrowed logical inputs and an independently owned prepared result.

use std::fmt;

use crate::value::original_flow::OriginalValueNode;

/// Logical data before ETF encoding, borrowing postorder node and edge arenas.
///
/// Node ranges address these pools. Every referenced child must precede its
/// parent. Foreign context flags are observations and confer no authority.
/// Local-fun uniqueness uses its signed low 32-bit source entry value;
/// repeated entry descriptors must agree on that identity.
#[derive(Clone, Copy, Debug)]
pub struct OriginalValueInput<'a> {
    /// Borrowed logical byte pool, without ETF or stored-value wrappers.
    pub bytes: &'a [u8],
    /// Borrowed typed nodes, with children preceding parents.
    pub nodes: &'a [OriginalValueNode],
    /// Borrowed child IDs addressed by compound node ranges.
    pub edges: &'a [usize],
    /// Logical root node ID.
    pub root: usize,
}

/// Positive independent ceilings for logical preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalValueEncodeLimits {
    /// Maximum borrowed logical byte-pool length.
    pub max_input_bytes: usize,
    /// Maximum supplied node count.
    pub max_nodes: usize,
    /// Maximum supplied edge count.
    pub max_edges: usize,
    /// Maximum logical nesting, including list aliases.
    pub max_depth: usize,
    /// Maximum complete selected output; the stored profile includes its wrapper.
    pub max_encoded_bytes: usize,
    /// Maximum continuous validation, comparison and emission work.
    pub max_work_units: usize,
    /// Maximum prospectively requested simultaneous output/scratch ownership.
    pub max_temporary_bytes: usize,
}

/// Complete prepared stored bytes with no remaining borrow into caller data.
#[derive(Debug)]
pub struct PreparedOriginalFlowValue {
    pub(in crate::value::original_flow::prepare) bytes: Vec<u8>,
    pub(in crate::value::original_flow::prepare) work_units: usize,
    pub(in crate::value::original_flow::prepare) temporary_bytes: usize,
}

impl PreparedOriginalFlowValue {
    /// Borrow the complete bytes consumed by explicit stored-value ingress.
    #[must_use]
    pub fn as_stored_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Move the same byte owner into stored-value ingress without copying.
    #[must_use]
    pub fn into_stored_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Continuous charged work through successful preparation.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work_units
    }

    /// Peak requested logical ownership, distinct from allocator capacity/RSS.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary_bytes
    }
}

/// Complete bare ETF bytes, distinct from the stored FSV2 value profile.
pub struct PreparedOriginalExternalTerm {
    pub(in crate::value::original_flow::prepare) bytes: Vec<u8>,
    pub(in crate::value::original_flow::prepare) work_units: usize,
    pub(in crate::value::original_flow::prepare) temporary_bytes: usize,
}

impl PreparedOriginalExternalTerm {
    /// Borrow the complete versioned external term, without a stored wrapper.
    #[must_use]
    pub fn as_external_term(&self) -> &[u8] {
        &self.bytes
    }

    /// Move the same external-term owner without copying or shrinking.
    #[must_use]
    pub fn into_external_term(self) -> Vec<u8> {
        self.bytes
    }

    /// Continuous logical work through successful preparation.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work_units
    }

    /// Peak requested logical owners, without native allocation/RSS guarantees.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary_bytes
    }
}

impl fmt::Debug for PreparedOriginalExternalTerm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedOriginalExternalTerm(<redacted>)")
    }
}

/// Input validity, operational admission and actual allocation failure differ.
#[derive(Debug)]
pub enum OriginalValueEncodeError {
    /// A reached local function needs the encoder's captured initialization PID.
    UnsupportedContext {
        /// Logical local-function node requiring the capture.
        node: usize,
    },
    /// Malformed logical nodes, graph references or duplicate map identities.
    InvalidInput,
    /// At least one required operational ceiling is zero.
    InvalidLimits,
    /// Checked work, size, depth or ownership exceeds its independent ceiling.
    Limit,
    /// An actual fallible allocation failed.
    ReserveFailed(std::collections::TryReserveError),
}

impl fmt::Display for OriginalValueEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedContext { node } => write!(
                formatter,
                "missing flow value encoder context at node {node}"
            ),
            Self::InvalidInput => formatter.write_str("invalid logical flow value"),
            Self::InvalidLimits => formatter.write_str("invalid value preparation limits"),
            Self::Limit => formatter.write_str("flow value preparation limit"),
            Self::ReserveFailed(error) => write!(formatter, "value preparation reserve: {error}"),
        }
    }
}

impl std::error::Error for OriginalValueEncodeError {}
