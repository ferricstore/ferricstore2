//! Bounded reply metadata; original payload and captured safety stay borrowed.

use std::fmt;

use super::super::original_entry::TermPosition;
use super::super::original_entry::indexed::OriginalPolicyCurrentIndex;
use super::super::{CompletePolicyEncodingError, OriginalEtfSafety, PolicySchemaLimits};
use crate::retention::ResolvedRetention;

/// Borrowed caller defaults, safety and optional native Map.new correspondence.
///
/// Construction grants no runtime capture, origin, validation or payment credit.
#[derive(Clone, Copy, Debug)]
pub struct OriginalPolicyReplyContext<'input> {
    pub(super) safety: OriginalEtfSafety<'input>,
    pub(super) defaults: ResolvedRetention,
    pub(super) encoded_binding: Option<&'input [u8]>,
    pub(super) states_order: Option<(u32, &'input [u32], &'input [u32])>,
    pub(super) additional_live: usize,
    pub(super) portable: bool,
}

impl<'input> OriginalPolicyReplyContext<'input> {
    /// Stores immutable inputs; extra live bytes can only increase admission.
    #[must_use]
    pub const fn new(
        safety: OriginalEtfSafety<'input>,
        defaults: ResolvedRetention,
        encoded_binding: Option<&'input [u8]>,
        states_order: Option<(u32, &'input [u32], &'input [u32])>,
        additional_live: usize,
    ) -> Self {
        Self {
            safety,
            defaults,
            encoded_binding,
            states_order,
            additional_live,
            portable: false,
        }
    }

    /// Uses the fixed portable server profile and reached actual binary-map order.
    /// This mathematical context supplies no storage or runtime-origin proof.
    #[must_use]
    pub fn portable(defaults: ResolvedRetention, additional_live: usize) -> Self {
        Self {
            safety: super::super::canonical_original_etf_safety(),
            defaults,
            encoded_binding: None,
            states_order: None,
            additional_live,
            portable: true,
        }
    }
}

/// Exact fixed caller diagnoses, separate from raised source exceptions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPolicyReplyRejection {
    /// A successful setter returned an undecodable policy envelope.
    AllocationFailed,
    /// A getter found invalid bytes, type or a nonbinary value.
    Corrupt,
    /// The supplied read originated from an unavailable shard.
    ShardUnavailable,
    /// The supplied read failed or returned an invalid reader result.
    ReadFailed,
}

impl OriginalPolicyReplyRejection {
    /// Borrows the original caller's response body without allocation.
    #[must_use]
    pub const fn body(self) -> &'static [u8] {
        self.text().as_bytes()
    }

    const fn text(self) -> &'static str {
        match self {
            Self::AllocationFailed => "ERR flow policy allocation failed",
            Self::Corrupt => "ERR flow policy is corrupt",
            Self::ShardUnavailable => "ERR flow policy shard not available",
            Self::ReadFailed => "ERR flow policy read failed",
        }
    }
}

impl fmt::Display for OriginalPolicyReplyRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.text())
    }
}
impl std::error::Error for OriginalPolicyReplyRejection {}

/// Operational refusals and the two reached ordinary source exception classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPolicyReplyError {
    /// A prospective debit, reservation or represented capability failed.
    Encoding(CompletePolicyEncodingError),
    /// The reached ordinary value has no Enumerable protocol.
    Enumerable,
    /// A callback shape or successful-prefix improper terminal has no clause.
    FunctionClause,
}

impl From<CompletePolicyEncodingError> for OriginalPolicyReplyError {
    fn from(value: CompletePolicyEncodingError) -> Self {
        Self::Encoding(value)
    }
}

impl fmt::Display for OriginalPolicyReplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoding(error) => fmt::Display::fmt(error, formatter),
            Self::Enumerable => formatter.write_str("Enumerable protocol is not implemented"),
            Self::FunctionClause => formatter.write_str("no matching source function clause"),
        }
    }
}
impl std::error::Error for OriginalPolicyReplyError {}

/// A generic semantic public reply over one original retained index.
///
/// The owner may outlive input descriptors, but borrows the original bytes,
/// request and captured safety donors. It neither emits ETF nor attests capture.
pub struct OriginalPolicyReply<'input> {
    pub(super) index: Option<OriginalPolicyCurrentIndex<'input, 'input>>,
    pub(super) requested_type: &'input [u8],
    pub(super) requested_state: Option<&'input [u8]>,
    pub(super) profile: PolicySchemaLimits,
    pub(super) resident_bound: usize,
    pub(super) records: Vec<PolicyReplyState<'input>>,
    pub(super) order: Vec<u32>,
    pub(super) capacity: usize,
    pub(super) version: PolicyReplyRef<'input>,
    pub(super) max_active: PolicyReplyRef<'input>,
    pub(super) selected: u32,
    pub(super) inline: [u8; 256],
    pub(super) coordinates: [(usize, usize); 4],
    pub(super) attributes: usize,
    pub(super) states: Option<TermPosition>,
}

impl<'input> OriginalPolicyReply<'input> {
    /// Creates an immutable view of the actual public root map.
    #[must_use]
    pub const fn root(&self) -> OriginalPolicyReplyValue<'_, 'input> {
        OriginalPolicyReplyValue {
            owner: self,
            reference: PolicyReplyRef::Map(PolicyReplyMap::Root),
        }
    }
    /// Reads the original generation; an absent getter has generation zero.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.index
            .as_ref()
            .map_or(0, |index| index.entry().generation())
    }
    /// Borrows the caller's requested type, independent of a setter's stored type.
    #[must_use]
    pub const fn requested_type(&self) -> &'input [u8] {
        self.requested_type
    }
    /// Distinguishes nil selection from every binary, including an empty binary.
    #[must_use]
    pub const fn requested_state(&self) -> Option<&'input [u8]> {
        self.requested_state
    }
}

/// A view borrowing its reply owner; large source values remain shared.
#[derive(Clone, Copy)]
pub struct OriginalPolicyReplyValue<'view, 'input> {
    pub(super) owner: &'view OriginalPolicyReply<'input>,
    pub(super) reference: PolicyReplyRef<'input>,
}

/// A constant-progress map iterator whose next operation is freshly charged.
pub struct OriginalPolicyReplyMapIter<'view, 'input> {
    pub(super) owner: &'view OriginalPolicyReply<'input>,
    pub(super) map: PolicyReplyMap,
    pub(super) phase: usize,
    pub(super) index: usize,
}

/// One borrowed key/value row, with the ordinary tuple representation.
pub type OriginalPolicyReplyMapEntry<'view, 'input> = (
    OriginalPolicyReplyValue<'view, 'input>,
    OriginalPolicyReplyValue<'view, 'input>,
);

#[derive(Clone, Copy)]
pub(super) enum PolicyReplyRef<'input> {
    Source(TermPosition),
    ListWindow { source: TermPosition, offset: u32 },
    Bytes(&'input [u8]),
    StaticAtom(&'static str),
    Unsigned(u64),
    Inline(u8),
    Attributes(u8),
    Map(PolicyReplyMap),
}

#[derive(Clone, Copy)]
pub(super) enum PolicyReplyMap {
    Root,
    States,
    State(u32),
    Retry(u32),
    Retention(u32),
    Backoff(u32),
    Source(TermPosition),
}

#[derive(Clone, Copy)]
pub(super) struct PolicyReplyState<'input> {
    pub(super) name: PolicyReplyRef<'input>,
    pub(super) source_ordinal: u32,
    pub(super) mode: bool,
    pub(super) max_retries: PolicyReplyRef<'input>,
    pub(super) exhausted_to: PolicyReplyRef<'input>,
    pub(super) ttl_ms: PolicyReplyRef<'input>,
    pub(super) history_max_events: PolicyReplyRef<'input>,
    pub(super) governance: PolicyReplyRef<'input>,
    pub(super) backoff: PolicyReplyBackoff,
}

impl PolicyReplyState<'_> {
    pub(super) fn defaults(defaults: ResolvedRetention) -> Self {
        Self {
            name: PolicyReplyRef::StaticAtom("nil"),
            source_ordinal: 0,
            mode: false,
            max_retries: PolicyReplyRef::Unsigned(3),
            exhausted_to: PolicyReplyRef::Bytes(b"failed"),
            ttl_ms: PolicyReplyRef::Unsigned(defaults.ttl_ms),
            history_max_events: PolicyReplyRef::Unsigned(u64::from(defaults.history_max_events)),
            governance: PolicyReplyRef::StaticAtom("nil"),
            backoff: PolicyReplyBackoff {
                type_map: None,
                local_map: None,
                default_mask: 0,
                len: 4,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct PolicyReplyBackoff {
    pub(super) type_map: Option<TermPosition>,
    pub(super) local_map: Option<TermPosition>,
    pub(super) default_mask: u8,
    pub(super) len: usize,
}
