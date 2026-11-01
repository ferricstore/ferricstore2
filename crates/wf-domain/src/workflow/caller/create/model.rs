//! Borrowed caller leaves, bounded normalized spines and measured observations.
use crate::metadata::{OriginalAttributes, OriginalMetadataInput as I, OriginalMetadataScalar};
use std::{borrow::Cow, fmt};

/// Actual captured helper context; these editable values confer no authority.
#[derive(Clone, Copy, Debug)]
pub struct OriginalCreateContext<'r, 'a> {
    /// Captured result of the original internal capability comparison.
    pub internal_allowed: bool,
    /// Actual original hot-history configuration, including invalid fallback classes.
    pub history_hot_maximum: I<'r, 'a>,
    /// Actual original total-history configuration, including invalid fallback classes.
    pub history_maximum: I<'r, 'a>,
}
impl Default for OriginalCreateContext<'_, '_> {
    fn default() -> Self {
        Self {
            internal_allowed: false,
            history_hot_maximum: I::Integer(10_000),
            history_maximum: I::Integer(1_000_000),
        }
    }
}

/// Independent prospective logical limits; not allocator capacity or RSS.
#[derive(Clone, Copy, Debug)]
pub struct OriginalCreateLimits {
    /// Complete captured descriptor/leaf occurrences, including ignored options.
    pub maximum_input_bytes: usize,
    /// Final requested logical ownership, excluding borrowed leaf backing.
    pub maximum_output_bytes: usize,
    /// Continuous input, normalization, comparison and admission work.
    pub maximum_work_units: usize,
    /// Simultaneous requested control, scratch and final ownership.
    pub maximum_temporary_bytes: usize,
}

/// A source observation retaining work even when the source refuses or raises.
#[derive(Debug)]
pub enum OriginalCreateOutcome<T> {
    /// All selected source normalization/admission observations succeed.
    Accepted {
        /// Ordinary normalized data, without execution authority.
        value: T,
        /// Continuous conservative work consumed.
        work_units: usize,
        /// Conservative peak simultaneous ownership.
        temporary_bytes: usize,
    },
    /// The original helper returns an error tuple.
    Rejected {
        /// Exact original source error bytes interpreted as UTF8.
        message: Cow<'static, str>,
        /// Work consumed before this source observation.
        work_units: usize,
        /// Peak simultaneous ownership before this observation.
        temporary_bytes: usize,
    },
    /// The original improper-list path raises an argument error.
    RaisedArgumentError {
        /// Work consumed before the source raise.
        work_units: usize,
        /// Peak simultaneous ownership before the raise.
        temporary_bytes: usize,
    },
}

/// Normalized schedule discovery data; no scheduler or internal grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalCreateScheduleMetadata<'a> {
    /// Exact normalized original atom spelling.
    pub kind: &'static str,
    /// Unchanged opaque nonempty target workflow type.
    pub target_type: &'a [u8],
    /// Original nonempty cron timezone; syntax is not validated by this helper.
    pub timezone: Option<&'a [u8]>,
}

pub(super) const NAMES: [&str; 24] = [
    "id",
    "type",
    "state",
    "parent_flow_id",
    "root_flow_id",
    "correlation_id",
    "idempotent",
    "now_ms",
    "run_at_ms",
    "retention_ttl_ms",
    "max_active_ms",
    "history_hot_max_events",
    "history_max_events",
    "priority",
    "payload",
    "payload_ref",
    "values",
    "value_refs",
    "drop_values",
    "override_values",
    "partition_key_explicit",
    "run_state",
    "worker",
    "lease_ms",
];

/// Original create or atomic-start caller; automatic partition resolution belongs to ports.
pub struct OriginalCreateCaller<'r, 'a> {
    pub(super) fields: [Option<I<'r, 'a>>; 24],
    pub(super) partition: Option<&'a [u8]>,
    pub(super) attributes: OriginalAttributes<'a>,
    pub(super) state_update: Vec<(&'a str, OriginalMetadataScalar<'a>)>,
    pub(super) schedule: Option<OriginalCreateScheduleMetadata<'a>>,
    pub(super) owned: usize,
}
impl<'r, 'a> OriginalCreateCaller<'r, 'a> {
    /// Borrows one normalized scalar/opaque field; missing remains absent.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<I<'r, 'a>> {
        if name == "state_meta_state" && !self.state_update.is_empty() {
            return self.fields[21].or(self.fields[2]);
        }
        NAMES
            .iter()
            .position(|n| *n == name)
            .and_then(|n| self.fields[n])
    }
    /// Caller-selected literal partition; absence requests the original auto bucket.
    #[must_use]
    pub const fn explicit_partition(&self) -> Option<&'a [u8]> {
        self.partition
    }
    /// Every normalized attribute, including the original empty-map result.
    #[must_use]
    pub const fn attributes(&self) -> &OriginalAttributes<'a> {
        &self.attributes
    }
    /// Normalized scalar-only update; an empty update creates no update fields.
    #[must_use]
    pub fn state_metadata_update(&self) -> &[(&'a str, OriginalMetadataScalar<'a>)] {
        &self.state_update
    }
    /// Normalized internal schedule discovery data.
    #[must_use]
    pub const fn schedule_metadata(&self) -> Option<OriginalCreateScheduleMetadata<'a>> {
        self.schedule
    }
    /// Complete field count excluding the unresolved automatic/literal partition field.
    #[must_use]
    pub fn field_count_without_partition(&self) -> usize {
        self.fields.iter().flatten().count()
            + 1
            + usize::from(self.schedule.is_some())
            + if self.state_update.is_empty() { 0 } else { 2 }
    }
    /// Conservative final caller ownership; backing leaves stay borrowed.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned
    }
}
impl fmt::Debug for OriginalCreateCaller<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalCreateCaller(<redacted>)")
    }
}

/// Operational capture/resource failures; source failures are measured outcomes.
#[derive(Debug)]
pub enum OriginalCreateError {
    /// A required resource ceiling is zero.
    InvalidLimits,
    /// Prospective input/output/work/temporary resources overflow or exceed a ceiling.
    Limit,
    /// Actual fallible reservation failure.
    ReserveFailed(std::collections::TryReserveError),
}
impl fmt::Display for OriginalCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid original create limits"),
            Self::Limit => f.write_str("original create resource limit"),
            Self::ReserveFailed(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for OriginalCreateError {}
