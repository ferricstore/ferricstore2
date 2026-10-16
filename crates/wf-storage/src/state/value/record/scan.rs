//! One borrowed record pass shared by point inspection and legacy ownership.

use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

use super::super::{
    StateValueError, add, copy, finish, flag, raw, read_u8, read_u64, read_u128, start,
};
use super::event;

/// Validated borrowed wire fields, without allocation or transaction authority.
///
/// The view borrows only its encoded donor. Materialization freshly admits its
/// complete logical owner size; scanning does not debit a transaction budget.
pub struct ScannedWorkflowRecord<'input> {
    tenant: &'input [u8],
    partition: &'input [u8],
    id: &'input [u8],
    workflow_type: &'input [u8],
    state: &'input [u8],
    ordering: OrderingMetadata,
    version: u64,
    fence: u64,
    attempts: u64,
    created: u64,
    updated: u64,
    next: Option<u64>,
    run_state: Option<&'input [u8]>,
    lease: Option<(&'input [u8], &'input [u8], u64)>,
    payload: Option<&'input [u8]>,
    result: Option<&'input [u8]>,
    error: Option<&'input [u8]>,
    latest: (EventKind, u64, u64),
    limits: AdmissionLimits,
    owned_bytes: usize,
}

impl ScannedWorkflowRecord<'_> {
    /// Reads the validated predecessor version without materializing an owner.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Reads 512 plus every variable byte returned by materialization.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }

    /// Consumes parsed fields after admitting their complete logical owner size.
    ///
    /// # Errors
    /// Rejects a zero/short owner cap, failed field allocation or typed restoration.
    pub fn into_owned(
        self,
        maximum_owned_bytes: usize,
    ) -> Result<StoredWorkflowRecord, StateValueError> {
        if maximum_owned_bytes == 0 {
            return Err(StateValueError::InvalidValue);
        }
        if self.owned_bytes > maximum_owned_bytes {
            return Err(StateValueError::Limit {
                bytes: self.owned_bytes,
                maximum: maximum_owned_bytes,
            });
        }
        let tenant =
            TenantId::new(copy(self.tenant)?).map_err(|_| StateValueError::InvalidValue)?;
        let partition =
            PartitionId::new(copy(self.partition)?).map_err(|_| StateValueError::InvalidValue)?;
        let lease = self
            .lease
            .map(|(worker, token, deadline)| {
                Ok::<_, StateValueError>(Lease {
                    worker: copy(worker)?,
                    token: LeaseToken::new(copy(token)?)
                        .map_err(|_| StateValueError::InvalidValue)?,
                    deadline: TimestampMillis::new(deadline),
                })
            })
            .transpose()?;
        let record = restore_record(
            WorkflowRecordData {
                id: RecordId::new(copy(self.id)?).map_err(|_| StateValueError::InvalidValue)?,
                workflow_type: copy(self.workflow_type)?,
                state: copy(self.state)?,
                version: self.version,
                fence: Fence::new(self.fence),
                attempts: self.attempts,
                created_at: TimestampMillis::new(self.created),
                updated_at: TimestampMillis::new(self.updated),
                next_run_at: self.next.map(TimestampMillis::new),
                run_state: self.run_state.map(copy).transpose()?,
                lease,
                payload: self.payload.map(copy).transpose()?,
                result: self.result.map(copy).transpose()?,
                error: self.error.map(copy).transpose()?,
                latest_kind: self.latest.0,
                latest_version: self.latest.1,
                latest_fence: Fence::new(self.latest.2),
            },
            self.limits,
        )
        .map_err(|_| StateValueError::InvalidValue)?;
        Ok(StoredWorkflowRecord {
            tenant,
            partition,
            record,
            ordering: self.ordering,
        })
    }

    fn validate(&self, profiled: bool) -> Result<usize, StateValueError> {
        if !nonempty(self.tenant, 8_192)
            || !nonempty(self.partition, 65_535)
            || !nonempty(self.id, 65_535)
            || self.ordering.priority > 2
            || self.workflow_type.is_empty()
            || self.state.is_empty()
            || self.version == 0
            || self.latest.1 != self.version
            || self.latest.2 != self.fence
            || self.created > MAX_EXACT_MILLIS
            || self.updated > MAX_EXACT_MILLIS
            || self.run_state.is_some_and(<[u8]>::is_empty)
            || (matches!(self.state, b"completed" | b"failed" | b"cancelled")
                && self.next.is_some()
                && self.latest.0 != EventKind::Created)
            || self.lease.is_some_and(|(worker, token, deadline)| {
                worker.is_empty()
                    || token.is_empty()
                    || deadline > MAX_EXACT_MILLIS
                    || self.state != b"running"
                    || self.next != Some(deadline)
            })
        {
            return Err(StateValueError::InvalidValue);
        }
        let mut core_bytes = self.id.len();
        for field in [
            Some(self.workflow_type),
            Some(self.state),
            self.run_state,
            self.payload,
            self.result,
            self.error,
            self.lease.map(|lease| lease.0),
            self.lease.map(|lease| lease.1),
        ]
        .into_iter()
        .flatten()
        {
            if field.len() > self.limits.inline_bytes() {
                return Err(profile_error(
                    profiled,
                    field.len(),
                    self.limits.inline_bytes(),
                ));
            }
            add(&mut core_bytes, field.len())?;
        }
        if core_bytes > self.limits.decision_bytes() {
            return Err(profile_error(
                profiled,
                core_bytes,
                self.limits.decision_bytes(),
            ));
        }
        Ok(core_bytes)
    }
}

/// Scans complete framing and binds all five expected point identity components.
///
/// # Errors
/// Rejects corrupt framing, foreign points, wire caps or represented semantics.
pub fn scan_record_point<'input>(
    bytes: &'input [u8],
    expected_scope: &RequestScope,
    expected_partition: &PartitionId,
    expected_id: &RecordId,
    limits: AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedWorkflowRecord<'input>, StateValueError> {
    scan(
        bytes,
        expected_scope.group(),
        expected_scope.recovery(),
        Some((expected_scope.tenant(), expected_partition, expected_id)),
        limits,
        maximum_wire_bytes,
        false,
    )
}

/// Scans the same complete point while reporting caller-profile budgets as limits.
/// Content, namespace and framing failures retain their legacy error origins.
/// # Errors
/// Rejects corrupt values, wire caps or precise inline/core profile exhaustion.
pub fn scan_record_point_profiled<'input>(
    bytes: &'input [u8],
    expected_scope: &RequestScope,
    expected_partition: &PartitionId,
    expected_id: &RecordId,
    limits: AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedWorkflowRecord<'input>, StateValueError> {
    scan(
        bytes,
        expected_scope.group(),
        expected_scope.recovery(),
        Some((expected_scope.tenant(), expected_partition, expected_id)),
        limits,
        maximum_wire_bytes,
        true,
    )
}

pub(super) fn scan_legacy<'input>(
    bytes: &'input [u8],
    group: &GroupId,
    recovery: &RecoveryId,
    limits: AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedWorkflowRecord<'input>, StateValueError> {
    scan(
        bytes,
        group,
        recovery,
        None,
        limits,
        maximum_wire_bytes,
        false,
    )
}

fn scan<'input>(
    bytes: &'input [u8],
    group: &GroupId,
    recovery: &RecoveryId,
    point: Option<(&TenantId, &PartitionId, &RecordId)>,
    limits: AdmissionLimits,
    maximum: usize,
    profiled: bool,
) -> Result<ScannedWorkflowRecord<'input>, StateValueError> {
    let mut offset = start(bytes, *b"FWSR", maximum)?;
    let actual_group = raw(bytes, &mut offset)?;
    let actual_recovery = raw(bytes, &mut offset)?;
    let tenant = raw(bytes, &mut offset)?;
    let partition = raw(bytes, &mut offset)?;
    let ordering = OrderingMetadata {
        priority: read_u8(bytes, &mut offset)?,
        state_enter_seq: read_u128(bytes, &mut offset)?,
    };
    let id = raw(bytes, &mut offset)?;
    let workflow_type = raw(bytes, &mut offset)?;
    let state = raw(bytes, &mut offset)?;
    let version = read_u64(bytes, &mut offset)?;
    let fence = read_u64(bytes, &mut offset)?;
    let attempts = read_u64(bytes, &mut offset)?;
    let created = read_u64(bytes, &mut offset)?;
    let updated = read_u64(bytes, &mut offset)?;
    let next = optional_time(bytes, &mut offset)?;
    let run_state = optional_raw(bytes, &mut offset)?;
    let lease = if flag(bytes, &mut offset)? {
        Some((
            raw(bytes, &mut offset)?,
            raw(bytes, &mut offset)?,
            read_u64(bytes, &mut offset)?,
        ))
    } else {
        None
    };
    let payload = optional_raw(bytes, &mut offset)?;
    let result = optional_raw(bytes, &mut offset)?;
    let error = optional_raw(bytes, &mut offset)?;
    let latest_tag = read_u8(bytes, &mut offset)?;
    let latest_version = read_u64(bytes, &mut offset)?;
    let latest_fence = read_u64(bytes, &mut offset)?;
    finish(bytes, offset)?;
    let latest = (event(latest_tag)?, latest_version, latest_fence);
    if actual_group != group.as_bytes()
        || actual_recovery != recovery.as_bytes()
        || point.is_some_and(|(expected_tenant, expected_partition, expected_id)| {
            tenant != expected_tenant.as_bytes()
                || partition != expected_partition.as_bytes()
                || id != expected_id.as_bytes()
        })
    {
        return Err(StateValueError::InvalidValue);
    }
    let mut scanned = ScannedWorkflowRecord {
        tenant,
        partition,
        id,
        workflow_type,
        state,
        ordering,
        version,
        fence,
        attempts,
        created,
        updated,
        next,
        run_state,
        lease,
        payload,
        result,
        error,
        latest,
        limits,
        owned_bytes: 512,
    };
    let core_bytes = scanned.validate(profiled)?;
    for length in [tenant.len(), partition.len(), core_bytes] {
        add(&mut scanned.owned_bytes, length)?;
    }
    Ok(scanned)
}

fn nonempty(bytes: &[u8], maximum: usize) -> bool {
    !bytes.is_empty() && bytes.len() <= maximum
}

fn optional_raw<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
) -> Result<Option<&'a [u8]>, StateValueError> {
    if flag(bytes, offset)? {
        Ok(Some(raw(bytes, offset)?))
    } else {
        Ok(None)
    }
}

fn optional_time(bytes: &[u8], offset: &mut usize) -> Result<Option<u64>, StateValueError> {
    if flag(bytes, offset)? {
        Ok(Some(read_u64(bytes, offset)?))
    } else {
        Ok(None)
    }
}

fn profile_error(profiled: bool, bytes: usize, maximum: usize) -> StateValueError {
    if profiled {
        StateValueError::Limit { bytes, maximum }
    } else {
        StateValueError::InvalidValue
    }
}
