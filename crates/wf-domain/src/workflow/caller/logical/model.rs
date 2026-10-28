//! Finite caller captures, explicit limits and independently owned output.

use std::fmt;

use crate::value::{OriginalValueEncodeError, OriginalValueEncodeLimits, OriginalValueInput};
use crate::workflow::CommittedCommand;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::caller::{MutationCallerError, MutationFieldValue, PublicMutationEvidence};

/// One scalar option or logical value arena, without runtime authority.
#[derive(Clone, Copy)]
pub enum LogicalMutationFieldValue<'a> {
    /// Existing scalar vocabulary; opaque positions encode primitive values.
    Scalar(MutationFieldValue<'a>),
    /// Explicit logical data and optional encoder initialization PID node.
    Value {
        /// Postorder logical data, without an already-encoded storage wrapper.
        input: OriginalValueInput<'a>,
        /// Same-arena captured initialization PID for reached local functions.
        init_pid: Option<usize>,
    },
}

impl fmt::Debug for LogicalMutationFieldValue<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Scalar(_) => "Scalar(<redacted>)",
            Self::Value { .. } => "Value(<redacted>)",
        })
    }
}

/// Original caller fields, with absence represented by an omitted option.
#[derive(Clone, Copy)]
pub struct LogicalMutationInput<'a> {
    /// Existing exact lowercase operation name.
    pub operation: &'a str,
    /// Original binary record identity.
    pub id: &'a [u8],
    /// Ordered local options; local values override common values.
    pub fields: &'a [(&'a str, LogicalMutationFieldValue<'a>)],
}

/// Independent profiles for one logical caller's preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalMutationLimits {
    /// Existing scalar/inline admission limits, not submission authority.
    pub admission: AdmissionLimits,
    /// Per-value logical preparation bounds.
    pub values: OriginalValueEncodeLimits,
    /// All original scalar/logical occurrences, including shadowed common data.
    pub max_raw_bytes: usize,
    /// Continuous scalar validation, copying and all selected value work.
    pub max_work_units: usize,
    /// Simultaneous requested control, output and preparation scratch ownership.
    pub max_temporary_bytes: usize,
}

/// Owned mutation data and original caller evidence, without submission power.
pub struct PreparedLogicalMutation {
    pub(in crate::workflow::caller::logical) command: CommittedCommand,
    pub(in crate::workflow::caller::logical) partition: Option<Vec<u8>>,
    pub(in crate::workflow::caller::logical) priority: u8,
    pub(in crate::workflow::caller::logical) evidence: PublicMutationEvidence,
    pub(in crate::workflow::caller::logical) work: usize,
    pub(in crate::workflow::caller::logical) temporary: usize,
}

impl PreparedLogicalMutation {
    /// Borrow the owned command whose opaque values contain complete stored bytes.
    #[must_use]
    pub const fn command(&self) -> &CommittedCommand {
        &self.command
    }

    /// Borrow an explicit partition; absence retains automatic-partition intent.
    #[must_use]
    pub fn partition(&self) -> Option<&[u8]> {
        self.partition.as_deref()
    }

    /// Read original create priority, zero for every other operation.
    #[must_use]
    pub const fn create_priority(&self) -> u8 {
        self.priority
    }

    /// Read original time/partition/retry presence, excluding generated context.
    #[must_use]
    pub const fn evidence(&self) -> PublicMutationEvidence {
        self.evidence
    }

    /// Continuous logical work charged through successful preparation.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }

    /// Peak requested logical ownership; not native capacity, frames or RSS.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }

    /// Move the same owners into explicit stored-value command construction.
    ///
    /// The outer codec resolves an absent automatic partition and binds scope,
    /// request, policy witnesses and preparation context. Binary8 construction
    /// would give these complete stored bytes a different meaning.
    #[must_use]
    pub fn into_stored_parts(
        self,
    ) -> (
        Option<Vec<u8>>,
        CommittedCommand,
        u8,
        PublicMutationEvidence,
    ) {
        (self.partition, self.command, self.priority, self.evidence)
    }
}

impl fmt::Debug for PreparedLogicalMutation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedLogicalMutation(<redacted>)")
    }
}

/// Source schema, logical value and operational refusal remain distinct.
#[derive(Debug)]
pub enum LogicalMutationError {
    /// Existing caller validation rejected the selected scalar shape.
    Caller(MutationCallerError),
    /// A selected opaque field failed logical value preparation.
    Value {
        /// Static public field name, never caller data.
        field: &'static str,
        /// Exact underlying preparation diagnosis.
        source: OriginalValueEncodeError,
    },
    /// A required aggregate or per-value profile contains a zero ceiling.
    InvalidLimits,
    /// Checked aggregate resources overflowed or exceeded their ceilings.
    Limit,
    /// An actual fallible metadata reservation failed.
    ReserveFailed(std::collections::TryReserveError),
}

impl fmt::Display for LogicalMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Caller(error) => write!(formatter, "logical mutation caller: {error}"),
            Self::Value { field, source } => {
                write!(formatter, "logical mutation {field}: {source}")
            }
            Self::InvalidLimits => formatter.write_str("invalid logical mutation limits"),
            Self::Limit => formatter.write_str("logical mutation resource limit"),
            Self::ReserveFailed(error) => write!(formatter, "logical mutation reserve: {error}"),
        }
    }
}

impl std::error::Error for LogicalMutationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Caller(error) => Some(error),
            Self::Value { source, .. } => Some(source),
            Self::ReserveFailed(error) => Some(error),
            Self::InvalidLimits | Self::Limit => None,
        }
    }
}

impl From<MutationCallerError> for LogicalMutationError {
    fn from(error: MutationCallerError) -> Self {
        Self::Caller(error)
    }
}
