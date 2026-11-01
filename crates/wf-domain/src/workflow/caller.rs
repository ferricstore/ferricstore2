//! Borrowed public mutation capture and deterministic source normalization.

use std::fmt;

use crate::policy::RetryOverride;

pub mod create;
mod input;
mod logical;
mod normalize;

pub mod many;

pub use input::measure_core_mutation_input;
pub use logical::{
    LogicalMutationError, LogicalMutationFieldValue, LogicalMutationInput, LogicalMutationLimits,
    PreparedLogicalMutation, prepare_logical_mutation,
};
pub use normalize::normalize_core_mutation;

/// One value in the explicitly supported transport-neutral capture vocabulary.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum MutationFieldValue<'a> {
    /// An explicitly supplied nil, distinct from an omitted field.
    Nil,
    /// A captured boolean.
    Bool(bool),
    /// A captured integer, before nonnegative/source-ceiling validation.
    Integer(i128),
    /// Arbitrary binary bytes, including a present empty value.
    Bytes(&'a [u8]),
    /// A borrowed, still revalidated retry override.
    Retry(&'a RetryOverride),
    /// A source value outside this finite capture schema.
    Unsupported,
}

impl fmt::Debug for MutationFieldValue<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => formatter.write_str("Nil"),
            Self::Bool(value) => formatter.debug_tuple("Bool").field(value).finish(),
            Self::Integer(value) => formatter.debug_tuple("Integer").field(value).finish(),
            Self::Bytes(_) => formatter.write_str("Bytes(<redacted>)"),
            Self::Retry(_) => formatter.write_str("Retry(<redacted>)"),
            Self::Unsupported => formatter.write_str("Unsupported"),
        }
    }
}

/// One caller item, without generated time, scope or authorization evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationInput<'a> {
    /// Exact lowercase source operation name.
    pub operation: &'a str,
    /// Original binary record identity.
    pub id: &'a [u8],
    /// Exact source option names; duplicates within this slice refuse.
    pub fields: &'a [(&'a str, MutationFieldValue<'a>)],
}

/// Normalized borrowed core effect; richer source effects remain unsupported.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum CoreMutation<'a> {
    /// Create an unleased record.
    Create {
        /// Exact workflow type.
        workflow_type: &'a [u8],
        /// Initial logical state.
        state: &'a [u8],
        /// Source priority band.
        priority: u8,
        /// Explicit or requested-now default deadline.
        run_at_ms: Option<u64>,
        /// Present replacement payload, including empty bytes.
        payload: Option<&'a [u8]>,
    },
    /// Transition between two logical states.
    Transition {
        /// Expected logical source state.
        from: &'a [u8],
        /// Destination logical state.
        to: &'a [u8],
        /// Expected fencing generation.
        fence: u64,
        /// Optional current ownership token.
        lease_token: Option<&'a [u8]>,
        /// Explicit or requested-now default deadline.
        run_at_ms: Option<u64>,
        /// Present replacement payload.
        payload: Option<&'a [u8]>,
    },
    /// Finish successfully with scalar core data.
    Complete {
        /// Current ownership token.
        lease_token: &'a [u8],
        /// Expected fencing generation.
        fence: u64,
        /// Present replacement payload.
        payload: Option<&'a [u8]>,
        /// Present result bytes.
        value: Option<&'a [u8]>,
    },
    /// Finish unsuccessfully with scalar core data.
    Fail {
        /// Current ownership token.
        lease_token: &'a [u8],
        /// Expected fencing generation.
        fence: u64,
        /// Present replacement payload.
        payload: Option<&'a [u8]>,
        /// Present error bytes.
        value: Option<&'a [u8]>,
    },
    /// Resolve source retry policy at authoritative fresh application.
    Retry {
        /// Current ownership token.
        lease_token: &'a [u8],
        /// Expected fencing generation.
        fence: u64,
        /// Explicit deadline; policy exhaustion still applies.
        run_at_ms: Option<u64>,
        /// Caller override, preserving even empty override presence.
        retry: Option<&'a RetryOverride>,
        /// Present replacement payload.
        payload: Option<&'a [u8]>,
        /// Present error bytes.
        error: Option<&'a [u8]>,
    },
    /// Extend current ownership.
    ExtendLease {
        /// Current ownership token.
        lease_token: &'a [u8],
        /// Expected fencing generation.
        fence: u64,
        /// Positive source duration.
        lease_ms: u64,
    },
    /// Cancel a record with explicit optional preconditions.
    Cancel {
        /// Expected fencing generation.
        fence: u64,
        /// Optional current ownership token.
        lease_token: Option<&'a [u8]>,
        /// Optional expected logical state.
        expected_state: Option<&'a [u8]>,
        /// Optional expected record version.
        expected_version: Option<u64>,
        /// Present cancellation reason bytes.
        reason: Option<&'a [u8]>,
    },
}

impl fmt::Debug for CoreMutation<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Create { .. } => "Create",
            Self::Transition { .. } => "Transition",
            Self::Complete { .. } => "Complete",
            Self::Fail { .. } => "Fail",
            Self::Retry { .. } => "Retry",
            Self::ExtendLease { .. } => "ExtendLease",
            Self::Cancel { .. } => "Cancel",
        })
    }
}

/// A mathematical normalized caller; it is not origin or admission authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedMutation<'a> {
    id: &'a [u8],
    partition: Option<&'a [u8]>,
    requested_now_ms: Option<u64>,
    operation: CoreMutation<'a>,
}

impl<'a> NormalizedMutation<'a> {
    /// Borrows the caller's unchanged binary identity.
    #[must_use]
    pub const fn id(&self) -> &'a [u8] {
        self.id
    }
    /// Reads an explicit partition; absence means semantic auto partition.
    #[must_use]
    pub const fn partition(&self) -> Option<&'a [u8]> {
        self.partition
    }
    /// Reads caller time, separately from any subsequently generated stamp.
    #[must_use]
    pub const fn requested_now_ms(&self) -> Option<u64> {
        self.requested_now_ms
    }
    /// Borrows the normalized effect without copying any payload.
    #[must_use]
    pub const fn operation(&self) -> &CoreMutation<'a> {
        &self.operation
    }
}

/// Scalar caller-presence data for the immutable public prepared profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicMutationEvidence {
    /// Caller requested time, excluding generated execution time.
    pub requested_now_ms: Option<u64>,
    /// The caller selected automatic rather than literal partitioning.
    pub auto_partition: bool,
    /// A retry override was supplied, including an empty override.
    pub retry_override_present: bool,
}

/// Finite caller refusal; diagnostics never include secret caller values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationCallerError {
    /// A configured logical resource profile is invalid.
    InvalidLimits,
    /// A captured option slice exceeds the finite field count.
    InvalidOptions,
    /// The record identity is empty, reserved or oversized.
    InvalidId,
    /// A required supported field was omitted.
    MissingField(&'static str),
    /// A supported field has an invalid scalar value.
    InvalidField(&'static str),
    /// One supplied field slice repeats a name.
    DuplicateField,
    /// A field name is outside the selected vocabulary.
    UnknownField,
    /// A known source feature or value is not represented by this profile.
    UnsupportedFeature(&'static str),
    /// An item conflicts with the caller family's explicit base partition.
    PartitionMismatch,
    /// The source many family requires a nonempty input.
    EmptyMany,
    /// The configured caller item count was exceeded.
    ItemLimit,
    /// Actual captured bytes exceed the caller budget.
    RawBytes,
    /// Checked size arithmetic overflowed.
    LengthOverflow,
    /// A fallible output reservation failed.
    Allocation,
}

impl fmt::Display for MutationCallerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(name) => write!(formatter, "missing mutation field: {name}"),
            Self::InvalidField(name) => write!(formatter, "invalid mutation field: {name}"),
            Self::UnsupportedFeature(name) => {
                write!(formatter, "unsupported mutation feature: {name}")
            }
            error => formatter.write_str(match error {
                Self::InvalidLimits => "invalid mutation limits",
                Self::InvalidOptions => "too many mutation fields",
                Self::InvalidId => "invalid mutation identity",
                Self::DuplicateField => "duplicate mutation field",
                Self::UnknownField => "unknown mutation field",
                Self::PartitionMismatch => "mutation partition mismatch",
                Self::EmptyMany => "empty mutation many input",
                Self::ItemLimit => "mutation item limit exceeded",
                Self::RawBytes => "mutation raw byte limit exceeded",
                Self::LengthOverflow => "mutation length overflow",
                Self::Allocation => "mutation allocation failed",
                Self::MissingField(_) | Self::InvalidField(_) | Self::UnsupportedFeature(_) => {
                    "invalid mutation"
                }
            }),
        }
    }
}

impl std::error::Error for MutationCallerError {}
