//! Borrowed caller spellings and a separate immutable comparison profile.

use super::ClaimCallerError;
use crate::workflow::admission::MAX_INLINE_BYTES;

/// Caller logical-state spelling before ordered alias normalization.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimStatesInput<'a> {
    /// All logical states.
    Any,
    /// One original caller binary, including the ASCII ANY alias.
    One(&'a [u8]),
    /// Original list occurrences, including repeats and empty binaries.
    Many(&'a [&'a [u8]]),
}

/// Caller partition spelling; scalar aliases differ from literal list keys.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimPartitionsInput<'a> {
    /// Automatic partition selection, without any fabricated bucket list.
    Auto,
    /// Any partition.
    Any,
    /// The global partition.
    Global,
    /// One original binary; ASCII AUTO, ANY and GLOBAL are scalar aliases.
    One(&'a [u8]),
    /// Original literal key occurrences; list aliases are ordinary keys.
    Many(&'a [&'a [u8]]),
}

/// Caller value projection spelling, including an explicit empty projection.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimValuesInput<'a> {
    /// Caller omitted the projection option.
    Omitted,
    /// Caller explicitly requested all values.
    All,
    /// Original name occurrences; an empty list remains explicit.
    Names(&'a [&'a [u8]]),
}

/// Exact public reply projection, without hydrated response ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimReplyMode {
    /// Full record response.
    Records,
    /// Job response.
    Jobs,
    /// Compact job response.
    Compact,
    /// Compact jobs with attributes.
    CompactAttrs,
    /// Compact jobs with state.
    CompactState,
    /// Compact jobs with state and attributes.
    CompactStateAttrs,
}

/// Normalized caller partition meaning, without routing or scope permission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimPartitionMode {
    /// Automatic selection remains a caller meaning.
    Auto,
    /// Any partition; shared-scope admission still rejects this meaning.
    Any,
    /// Global partition; shared-scope admission still rejects this meaning.
    Global,
    /// Explicit literal keys, including an explicit empty list.
    Only,
}

/// Normalized value projection meaning, independent of payload policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimValuesMode {
    /// Omitted projection option.
    Omitted,
    /// Explicit all-values request.
    All,
    /// Explicit ordered names, including an explicit empty list.
    Only,
}

/// Borrowed public caller values after an outer parser resolves spellings.
///
/// Malformed DTOs, unknown or duplicate fields and mutually exclusive spellings
/// remain outer parser responsibilities. Empty and invalid business values stay
/// representable for comparison; this value grants no time or scope authority.
#[derive(Clone, Copy)]
pub struct ClaimDueInput<'a> {
    /// Exact caller workflow type.
    pub workflow_type: &'a [u8],
    /// Exact caller worker identity.
    pub worker: &'a [u8],
    /// Caller logical states.
    pub states: ClaimStatesInput<'a>,
    /// Caller partition meaning or literal keys.
    pub partitions: ClaimPartitionsInput<'a>,
    /// Caller value projection.
    pub values: ClaimValuesInput<'a>,
    /// Optional caller lease duration.
    pub lease_ms: Option<u64>,
    /// Optional caller claim count.
    pub limit: Option<u64>,
    /// Optional caller priority; omission remains distinct.
    pub priority: Option<u64>,
    /// Optional caller reclaim ratio.
    pub reclaim_ratio: Option<u64>,
    /// Optional caller expired-lease reclamation flag.
    pub reclaim_expired: Option<bool>,
    /// Optional caller response mode.
    pub reply_mode: Option<ClaimReplyMode>,
    /// Optional legacy full-payload spelling.
    pub full: Option<bool>,
    /// Optional payload flag, overriding the legacy full spelling.
    pub payload: Option<bool>,
    /// Optional payload cap; absence cannot be filled from current policy.
    pub payload_max_bytes: Option<u64>,
    /// Optional exact caller governance scope, independently of shard presence.
    pub governance_limit_scope: Option<&'a [u8]>,
    /// Optional caller governance shard, independently of scope presence.
    pub governance_shard_id: Option<u64>,
    /// Optional source caller now value, used for identity only, never as a clock.
    pub requested_now_ms: Option<u64>,
    /// Optional wait duration; separate attempt semantics, excluded from intent.
    pub block_ms: Option<u64>,
}

impl<'a> ClaimDueInput<'a> {
    /// Creates Any/Auto/Omitted inputs with all option fields absent.
    #[must_use]
    pub const fn new(workflow_type: &'a [u8], worker: &'a [u8]) -> Self {
        Self {
            workflow_type,
            worker,
            states: ClaimStatesInput::Any,
            partitions: ClaimPartitionsInput::Auto,
            values: ClaimValuesInput::Omitted,
            lease_ms: None,
            limit: None,
            priority: None,
            reclaim_ratio: None,
            reclaim_expired: None,
            reply_mode: None,
            full: None,
            payload: None,
            payload_max_bytes: None,
            governance_limit_scope: None,
            governance_shard_id: None,
            requested_now_ms: None,
            block_ms: None,
        }
    }
}

/// Immutable preparation profile, separate from source admission and apply caps.
///
/// Limits bound raw occurrence counts and logical byte reads. They certify
/// neither native allocation/RSS nor successful intent or source admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimCallerLimits {
    components: usize,
    component_bytes: usize,
    raw_bytes: usize,
}

impl Default for ClaimCallerLimits {
    fn default() -> Self {
        Self {
            components: 100_000,
            component_bytes: 1_048_576,
            raw_bytes: 16 * 1_024 * 1_024,
        }
    }
}

impl ClaimCallerLimits {
    /// Validates positive limits and the selected count/component hard ceilings.
    ///
    /// # Errors
    /// Rejects zero, counts above 100000, or components above 512MiB.
    pub const fn new(
        max_components: usize,
        max_component_bytes: usize,
        max_raw_bytes: usize,
    ) -> Result<Self, ClaimCallerError> {
        if max_components == 0
            || max_components > 100_000
            || max_component_bytes == 0
            || max_component_bytes > MAX_INLINE_BYTES
            || max_raw_bytes == 0
        {
            Err(ClaimCallerError::InvalidLimits)
        } else {
            Ok(Self {
                components: max_components,
                component_bytes: max_component_bytes,
                raw_bytes: max_raw_bytes,
            })
        }
    }

    /// Maximum raw occurrences of each individual state, key or value list.
    #[must_use]
    pub const fn max_components(self) -> usize {
        self.components
    }

    /// Maximum actual length of each original caller binary.
    #[must_use]
    pub const fn max_component_bytes(self) -> usize {
        self.component_bytes
    }

    /// Maximum checked occurrence-byte sum before any deduplication.
    #[must_use]
    pub const fn max_raw_bytes(self) -> usize {
        self.raw_bytes
    }
}
