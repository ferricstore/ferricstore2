//! Flat owned value data and explicit compatibility/resource inputs.

use std::ops::Range;

/// Owned encoded bytes and flat typed structural spines.
///
/// This document carries no admission, storage availability or runtime authority.
#[derive(Debug)]
pub struct OriginalValueDocument {
    pub(in crate::value::original_flow) encoded: Vec<u8>,
    pub(in crate::value::original_flow) nodes: Vec<OriginalValueNode>,
    pub(in crate::value::original_flow) edges: Vec<usize>,
    pub(in crate::value::original_flow) root: usize,
}

impl OriginalValueDocument {
    /// Owned buffer capacities, excluding the enclosing static document descriptor.
    /// This is logical buffer ownership, without allocator metadata or RSS.
    #[must_use]
    pub fn owned_buffer_bytes(&self) -> Option<usize> {
        self.nodes
            .capacity()
            .checked_mul(std::mem::size_of::<OriginalValueNode>())?
            .checked_add(
                self.edges
                    .capacity()
                    .checked_mul(std::mem::size_of::<usize>())?,
            )?
            .checked_add(self.encoded.capacity())
    }
    pub(crate) fn as_input(&self) -> crate::value::OriginalValueInput<'_> {
        crate::value::OriginalValueInput {
            bytes: &self.encoded,
            nodes: &self.nodes,
            edges: &self.edges,
            root: self.root,
        }
    }
    pub(crate) fn into_encoded(self) -> Vec<u8> {
        self.encoded
    }
    /// Borrow the complete selected decoder input, including any stored wrapper.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }

    /// Borrow the root when its node ID is in bounds.
    #[must_use]
    pub fn root(&self) -> Option<&OriginalValueNode> {
        self.nodes.get(self.root)
    }

    /// Borrow an in-bounds node ID.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<&OriginalValueNode> {
        self.nodes.get(index)
    }

    /// Borrow an in-bounds, non-reversed range of stored bytes.
    #[must_use]
    pub fn byte_range(&self, range: Range<usize>) -> Option<&[u8]> {
        self.encoded.get(range)
    }

    /// Borrow an in-bounds, non-reversed range of node IDs.
    #[must_use]
    pub fn edge_range(&self, range: Range<usize>) -> Option<&[usize]> {
        self.edges.get(range)
    }
}

/// One typed value; byte and edge ranges address its owning document.
#[derive(Debug)]
pub enum OriginalValueNode {
    /// The empty proper list, distinct from the atom `nil`.
    EmptyList,
    /// A normalized mathematical integer in the i64 range.
    Integer(i64),
    /// A wider nonzero integer with trimmed little-endian magnitude bytes.
    BigInteger {
        /// Whether the nonzero magnitude is negative.
        negative: bool,
        /// Little-endian magnitude bytes in the encoded owner.
        digits: Range<usize>,
    },
    /// Finite binary64 bits, preserving the sign of zero.
    FloatBits(u64),
    /// An atom spelling without interning or runtime authority.
    Atom {
        /// Encoded atom name bytes.
        bytes: Range<usize>,
        /// Whether the name uses UTF8 rather than Latin1.
        utf8: bool,
    },
    /// An opaque binary value.
    Binary {
        /// Payload bytes in the encoded owner.
        bytes: Range<usize>,
    },
    /// A nonempty bitstring with the valid bit count of its final byte.
    BitString {
        /// Payload bytes in the encoded owner.
        bytes: Range<usize>,
        /// Valid high bits in the final byte, from one through eight.
        last_bits: u8,
    },
    /// Ordered tuple children.
    Tuple {
        /// Child node IDs in the edge arena.
        children: Range<usize>,
    },
    /// A list spine and its typed proper or improper tail.
    List {
        /// Head node IDs in the edge arena.
        heads: Range<usize>,
        /// Node ID of the list tail.
        tail: usize,
    },
    /// Parsed map order, with alternating key and value IDs.
    Map {
        /// An even-length range of key/value node IDs.
        entries: Range<usize>,
    },
    /// A foreign process identity as data, never an executable resource.
    ForeignPid {
        /// Node atom ID.
        node: usize,
        /// Process ID word.
        id: u32,
        /// Process serial word.
        serial: u32,
        /// Node creation word.
        creation: u32,
        /// Captured equality with the explicit local context.
        local_to_context: bool,
    },
    /// A foreign port identity as data.
    ForeignPort {
        /// Node atom ID.
        node: usize,
        /// Port identity word.
        id: u64,
        /// Node creation word.
        creation: u32,
        /// Captured equality with the explicit local context.
        local_to_context: bool,
    },
    /// A foreign reference identity as data, without a resource capability.
    ForeignReference {
        /// Node atom ID.
        node: usize,
        /// Node creation word.
        creation: u32,
        /// Encoded big-endian u32 identity words in the byte owner.
        words: Range<usize>,
        /// Captured equality with the explicit local context.
        local_to_context: bool,
    },
    /// A foreign external function identity, not a Rust callable.
    ForeignExport {
        /// Module atom ID.
        module: usize,
        /// Function atom ID.
        function: usize,
        /// Function arity.
        arity: u32,
    },
    /// A foreign local function identity and captured free values as data.
    ForeignLocalFun {
        /// Module atom ID.
        module: usize,
        /// Function arity.
        arity: u8,
        /// Function identity index.
        index: i32,
        /// Function identity digest.
        uniq: [u8; 16],
        /// Legacy index field.
        old_index: i64,
        /// Legacy uniqueness field.
        old_uniq: i64,
        /// Creator process node ID.
        creator: usize,
        /// Free-value node IDs in the edge arena.
        free: Range<usize>,
    },
}

/// Explicit immutable compatibility captures, not a live-runtime certificate.
#[derive(Clone, Copy, Debug)]
pub struct OriginalValueContext<'a> {
    /// Captured existing atom names; `None` means unavailable, not an empty set.
    pub safe_atoms: Option<&'a [&'a str]>,
    /// Captured active exports as module, function and arity triples.
    pub active_exports: Option<&'a [(&'a str, &'a str, u32)]>,
    /// Captured local node name and creation for identity comparison.
    pub local_node: Option<(&'a str, u32)>,
    /// Captured local function identities, without executable owners.
    pub local_functions: Option<&'a [OriginalValueFunctionIdentity<'a>]>,
}

/// A captured canonical function identity, without code or a callback.
#[derive(Clone, Copy, Debug)]
pub struct OriginalValueFunctionIdentity<'a> {
    /// Module name.
    pub module: &'a str,
    /// Canonical function index.
    pub index: i32,
    /// Function arity.
    pub arity: u8,
    /// Canonical function digest.
    pub uniq: [u8; 16],
    /// Legacy uniqueness field.
    pub old_uniq: i32,
}

/// Positive operational bounds, separate from stored-size omission semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalValueLimits {
    /// Maximum complete stored input length AND moved input capacity in bytes.
    pub max_encoded_bytes: usize,
    /// Maximum returned typed nodes.
    pub max_nodes: usize,
    /// Maximum returned edge IDs and prospectively requested edge spines.
    pub max_edges: usize,
    /// Maximum encoded typed-subterm nesting, including zero-head list wrappers.
    pub max_depth: usize,
    /// Maximum admitted inspection, traversal and comparison work units.
    pub max_work_units: usize,
}

/// Source corruption is distinct from unavailable compatibility and resources.
#[derive(Debug)]
pub enum OriginalFlowValueError {
    /// Malformed bytes or a known-absent safe atom/export.
    InvalidFlowValue,
    /// A reached source class requires an unavailable captured context.
    UnsupportedContext {
        /// Reached ETF tag.
        tag: u8,
        /// Tag offset in the complete stored input.
        offset: usize,
    },
    /// A valid source class belongs to a currently unsupported profile.
    UnsupportedProfile {
        /// Reached ETF tag.
        tag: u8,
        /// Tag offset in the complete stored input.
        offset: usize,
    },
    /// An invalid or arithmetically incompatible operational profile.
    InvalidLimits,
    /// A checked resource count overflowed or exceeded its operational bound.
    Limit,
    /// An actual fallible reservation failed.
    ReserveFailed(std::collections::TryReserveError),
}
