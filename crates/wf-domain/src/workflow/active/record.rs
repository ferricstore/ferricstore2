//! One immutable rich owner with pure move construction and borrowed validation.

use super::releases::validate_reservation;
use super::values::validate_refs;
use super::{ActiveValueError, ActiveValueRefs, CapturedLifecycle, GovernanceReservation};
use crate::lineage::{ChildGroup, ParentLink, validate_group};
use crate::retention::MAX_RETENTION_MILLIS;
use crate::workflow::admission::{AdmissionError, AdmissionLimits, record_bytes};
use crate::workflow::indexed::OrderingMetadata;
use crate::workflow::{MAX_EXACT_MILLIS, WorkflowRecord};
use std::collections::BTreeMap;
use wf_types::bounds::MAX_RECORD_ID_BYTES;
use wf_types::identity::PartitionId;
use wf_types::version::TimestampMillis;

/// Immutable actual history/extension bytes; absent and empty remain distinct.
#[derive(Clone, Eq, PartialEq)]
pub struct ActiveCarry {
    /// Preserved rewind identity.
    pub rewound_to_event_id: Option<Vec<u8>>,
    /// Actual named-reference history bytes.
    pub named_refs_history: Option<Vec<u8>>,
    /// Actual attribute history bytes.
    pub attributes_history: Option<Vec<u8>>,
    /// Actual state-metadata history bytes.
    pub state_meta_history: Option<Vec<u8>>,
    /// Static supported extension bytes; outer codec excludes all modeled fields.
    pub record_extensions: Vec<u8>,
}
/// Complete owned input to the pure rich-value constructor.
pub struct ActiveRecordData {
    /// Canonical core value.
    pub core: WorkflowRecord,
    /// Actual partition identity.
    pub partition: PartitionId,
    /// Captured priority and logical state-entry sequence.
    pub ordering: OrderingMetadata,
    /// Immutable resolved creation-time lifecycle.
    pub captured: CapturedLifecycle,
    /// Actual transient TTL, cleared by timeout/resolution.
    pub ttl_ms: Option<u64>,
    /// Actual terminal-retention score, independent of state-key expiry.
    pub terminal_retention_until: Option<TimestampMillis>,
    /// Historical actual backlink.
    pub parent: Option<ParentLink>,
    /// Optional root metadata, including empty bytes.
    pub root_id: Option<Vec<u8>>,
    /// Optional correlation metadata.
    pub correlation_id: Option<Vec<u8>>,
    /// Byte-ordered historical captured groups.
    pub child_groups: BTreeMap<Vec<u8>, ChildGroup>,
    /// Actual refs with outer replicated-availability obligations.
    pub values: ActiveValueRefs,
    /// Immutable actual history/static extensions.
    pub carry: ActiveCarry,
    /// Actual local reservation metadata.
    pub governance: Option<GovernanceReservation>,
}
/// Sealed canonical rich owner; immutable getters do not establish storage provenance.
#[derive(Clone, Eq, PartialEq)]
pub struct ActiveRecord {
    pub(super) core: WorkflowRecord,
    pub(super) partition: PartitionId,
    pub(super) ordering: OrderingMetadata,
    pub(super) captured: CapturedLifecycle,
    pub(super) ttl_ms: Option<u64>,
    pub(super) terminal_retention_until: Option<TimestampMillis>,
    pub(super) parent: Option<ParentLink>,
    pub(super) root_id: Option<Vec<u8>>,
    pub(super) correlation_id: Option<Vec<u8>>,
    pub(super) child_groups: BTreeMap<Vec<u8>, ChildGroup>,
    pub(super) values: ActiveValueRefs,
    pub(super) carry: ActiveCarry,
    pub(super) governance: Option<GovernanceReservation>,
}
impl ActiveRecord {
    /// Borrows the canonical core.
    #[must_use]
    pub const fn core(&self) -> &WorkflowRecord {
        &self.core
    }
    /// Borrows its actual partition.
    #[must_use]
    pub const fn partition(&self) -> &PartitionId {
        &self.partition
    }
    /// Reads captured ordering metadata.
    #[must_use]
    pub const fn ordering(&self) -> OrderingMetadata {
        self.ordering
    }
    /// Reads captured lifecycle scalars.
    #[must_use]
    pub const fn captured(&self) -> CapturedLifecycle {
        self.captured
    }
    /// Reads actual optional transient TTL.
    #[must_use]
    pub const fn ttl_ms(&self) -> Option<u64> {
        self.ttl_ms
    }
    /// Reads actual optional terminal-retention deadline.
    #[must_use]
    pub const fn terminal_retention_until(&self) -> Option<TimestampMillis> {
        self.terminal_retention_until
    }
    /// Borrows the actual historical backlink.
    #[must_use]
    pub const fn parent(&self) -> Option<&ParentLink> {
        self.parent.as_ref()
    }
    /// Borrows optional root metadata.
    #[must_use]
    pub fn root_id(&self) -> Option<&[u8]> {
        self.root_id.as_deref()
    }
    /// Borrows optional correlation metadata.
    #[must_use]
    pub fn correlation_id(&self) -> Option<&[u8]> {
        self.correlation_id.as_deref()
    }
    /// Borrows all captured historical groups.
    #[must_use]
    pub const fn child_groups(&self) -> &BTreeMap<Vec<u8>, ChildGroup> {
        &self.child_groups
    }
    /// Borrows actual value refs.
    #[must_use]
    pub const fn values(&self) -> &ActiveValueRefs {
        &self.values
    }
    /// Borrows preserved history/static bytes.
    #[must_use]
    pub const fn carry(&self) -> &ActiveCarry {
        &self.carry
    }
    /// Borrows actual reservation metadata.
    #[must_use]
    pub const fn governance(&self) -> Option<&GovernanceReservation> {
        self.governance.as_ref()
    }
}

/// Moves complete owners after pure logical/scalar/count/actual-byte validation.
///
/// # Errors
/// Rejects inconsistent represented lifecycle/lineage/refs or selected byte limits.
pub fn restore_active(
    data: ActiveRecordData,
    limits: AdmissionLimits,
) -> Result<ActiveRecord, ActiveValueError> {
    let record = ActiveRecord {
        core: data.core,
        partition: data.partition,
        ordering: data.ordering,
        captured: data.captured,
        ttl_ms: data.ttl_ms,
        terminal_retention_until: data.terminal_retention_until,
        parent: data.parent,
        root_id: data.root_id,
        correlation_id: data.correlation_id,
        child_groups: data.child_groups,
        values: data.values,
        carry: data.carry,
        governance: data.governance,
    };
    validate_active_data(&record, limits)?;
    Ok(record)
}

pub(super) fn validate_active_data(
    record: &ActiveRecord,
    limits: AdmissionLimits,
) -> Result<usize, ActiveValueError> {
    let terminal = matches!(record.core.state(), b"completed" | b"failed" | b"cancelled");
    if record.ordering.priority > 2
        || terminal != record.terminal_retention_until.is_some()
        || record
            .terminal_retention_until
            .is_some_and(|t| t.get() > MAX_EXACT_MILLIS)
        || record
            .ttl_ms
            .is_some_and(|n| n == 0 || n > MAX_RETENTION_MILLIS)
        || record.child_groups.len() > limits.batch_items()
    {
        return Err(ActiveValueError::InvalidExtensions);
    }
    let overflow = || ActiveValueError::Admission(AdmissionError::LengthOverflow);
    let mut bytes = record_bytes(&record.core, limits).map_err(ActiveValueError::Admission)?;
    bytes = bytes
        .checked_add(record.partition.as_bytes().len())
        .ok_or_else(overflow)?;
    for value in [
        record.parent.as_ref().map(|p| p.id.as_slice()),
        record.root_id(),
        record.correlation_id(),
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > MAX_RECORD_ID_BYTES {
            return Err(ActiveValueError::InvalidLineage);
        }
        limits
            .check_value(value)
            .map_err(ActiveValueError::Admission)?;
        bytes = bytes.checked_add(value.len()).ok_or_else(overflow)?;
    }
    if let Some(partition) = record.parent.as_ref().and_then(|p| p.partition.as_ref()) {
        bytes = bytes
            .checked_add(partition.as_bytes().len())
            .ok_or_else(overflow)?;
    }
    for (id, group) in &record.child_groups {
        limits
            .check_value(id)
            .map_err(ActiveValueError::Admission)?;
        let group_bytes = validate_group(group, limits)?;
        bytes = bytes
            .checked_add(id.len())
            .and_then(|n| n.checked_add(group_bytes))
            .ok_or_else(overflow)?;
    }
    bytes = bytes
        .checked_add(validate_refs(&record.values, limits)?)
        .ok_or_else(overflow)?;
    for value in [
        record.carry.rewound_to_event_id.as_deref(),
        record.carry.named_refs_history.as_deref(),
        record.carry.attributes_history.as_deref(),
        record.carry.state_meta_history.as_deref(),
        Some(record.carry.record_extensions.as_slice()),
    ]
    .into_iter()
    .flatten()
    {
        limits
            .check_value(value)
            .map_err(ActiveValueError::Admission)?;
        bytes = bytes.checked_add(value.len()).ok_or_else(overflow)?;
    }
    if let Some(reservation) = &record.governance {
        bytes = bytes
            .checked_add(validate_reservation(reservation)?)
            .ok_or_else(overflow)?;
    }
    limits
        .check_total(bytes)
        .map_err(ActiveValueError::Admission)?;
    Ok(bytes)
}
