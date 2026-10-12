//! Logical value identities; exact physical encoding and availability stay outer.

use super::ActiveValueError;
use crate::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::TimestampMillis;

/// Kind of a separately versioned value.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ActiveValueKind {
    /// Original payload.
    Payload,
    /// Successful result.
    Result,
    /// Error value.
    Error,
}
/// Actual reference ownership; this value does not certify stored availability.
#[derive(Clone, Eq, PartialEq)]
pub enum ActiveValueRef {
    /// Preserved opaque source reference, including explicitly empty bytes.
    Opaque(Vec<u8>),
    /// Scoped versioned identity without a fabricated durable key.
    Versioned {
        /// Actual partition component.
        partition: PartitionId,
        /// Actual record component.
        id: RecordId,
        /// Value dimension.
        kind: ActiveValueKind,
        /// Positive creating record version.
        version: u64,
    },
}
/// Optional actual refs; absent differs from an empty opaque owner.
#[derive(Clone, Eq, PartialEq)]
pub struct ActiveValueRefs {
    /// Original payload ref.
    pub payload: Option<ActiveValueRef>,
    /// Successful result ref.
    pub result: Option<ActiveValueRef>,
    /// Error ref.
    pub error: Option<ActiveValueRef>,
}
/// Typed max-active error body; the static source reason is `max_active_ms`.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TimeoutError {
    /// Captured exceeded duration.
    pub max_active_ms: u64,
}
/// New error value output at its exact new record version.
pub struct TimeoutValueWrite {
    /// Local scoped Versioned Error identity.
    pub reference: ActiveValueRef,
    /// Structured body for outer encoding.
    pub body: TimeoutError,
    /// Terminal expiry applies to this new value, not the state or older values.
    pub expires_at: Option<TimestampMillis>,
}

pub(super) fn validate_refs(
    refs: &ActiveValueRefs,
    limits: AdmissionLimits,
) -> Result<usize, ActiveValueError> {
    let mut n = 0_usize;
    for reference in [
        refs.payload.as_ref(),
        refs.result.as_ref(),
        refs.error.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        n = n
            .checked_add(value_ref_bytes(reference, limits)?)
            .ok_or(ActiveValueError::Admission(AdmissionError::LengthOverflow))?;
    }
    Ok(n)
}

pub(super) fn timeout_value_ref(record: &super::ActiveRecord, version: u64) -> ActiveValueRef {
    ActiveValueRef::Versioned {
        partition: record.partition().clone(),
        id: record.core().id().clone(),
        kind: ActiveValueKind::Error,
        version,
    }
}

pub(crate) fn value_ref_bytes(
    reference: &ActiveValueRef,
    limits: AdmissionLimits,
) -> Result<usize, ActiveValueError> {
    match reference {
        ActiveValueRef::Opaque(bytes) => {
            limits
                .check_value(bytes)
                .map_err(ActiveValueError::Admission)?;
            Ok(bytes.len())
        }
        ActiveValueRef::Versioned {
            partition,
            id,
            version,
            ..
        } => {
            if *version == 0 {
                return Err(ActiveValueError::InvalidRefs);
            }
            partition
                .as_bytes()
                .len()
                .checked_add(id.as_bytes().len())
                .ok_or(ActiveValueError::Admission(AdmissionError::LengthOverflow))
        }
    }
}
