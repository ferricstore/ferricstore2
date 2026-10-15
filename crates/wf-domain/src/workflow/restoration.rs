//! Pure owner-moving restoration; no encoded or historical provenance is inferred.

use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use crate::history::{EventKind, LatestEvent};
use crate::lease::Lease;
use crate::value::InputField;

use super::active::ActiveValueError;
use super::admission::{AdmissionLimits, record_bytes};
use super::validation::{require_nonempty, terminal_state, validate_run_at};
use super::{DomainError, MAX_EXACT_MILLIS, WorkflowRecord};

/// Explicit canonical fields and full latest-event witness supplied by an outer decoder.
pub struct WorkflowRecordData {
    /// Opaque scoped record component.
    pub id: RecordId,
    /// Opaque workflow type.
    pub workflow_type: Vec<u8>,
    /// Physical state, including arbitrary nonterminal targets.
    pub state: Vec<u8>,
    /// Current record version.
    pub version: u64,
    /// Current fencing generation.
    pub fence: Fence,
    /// Current retry attempts.
    pub attempts: u64,
    /// Original creation time.
    pub created_at: TimestampMillis,
    /// Last mutation time.
    pub updated_at: TimestampMillis,
    /// Actual optional due or lease time; absence remains absent.
    pub next_run_at: Option<TimestampMillis>,
    /// Actual optional logical running state.
    pub run_state: Option<Vec<u8>>,
    /// Actual optional lease; physical running does not imply presence.
    pub lease: Option<Lease>,
    /// Inline payload, preserving absent versus empty.
    pub payload: Option<Vec<u8>>,
    /// Inline result.
    pub result: Option<Vec<u8>>,
    /// Inline error.
    pub error: Option<Vec<u8>>,
    /// Explicit event label, never inferred from state.
    pub latest_kind: EventKind,
    /// Explicit event version, checked against current version.
    pub latest_version: u64,
    /// Explicit event fence, checked against current fence.
    pub latest_fence: Fence,
}

/// Validates logical consistency and moves every byte owner without cloning.
///
/// `InvalidExtensions` includes restored core/witness consistency. This operation
/// certifies neither a committed codec nor durable history/snapshot provenance.
///
/// # Errors
/// Rejects invalid represented values, witness consistency or selected byte limits.
pub fn restore_record(
    data: WorkflowRecordData,
    limits: AdmissionLimits,
) -> Result<WorkflowRecord, ActiveValueError> {
    validate_record_data(&data)?;
    let record = WorkflowRecord {
        id: data.id,
        workflow_type: data.workflow_type,
        state: data.state,
        version: data.version,
        fence: data.fence,
        attempts: data.attempts,
        created_at: data.created_at,
        updated_at: data.updated_at,
        next_run_at: data.next_run_at,
        run_state: data.run_state,
        lease: data.lease,
        payload: data.payload,
        result: data.result,
        error: data.error,
        latest_event: LatestEvent {
            kind: data.latest_kind,
            version: data.latest_version,
            fence: data.latest_fence,
        },
    };
    record_bytes(&record, limits).map_err(ActiveValueError::Admission)?;
    Ok(record)
}

fn validate_record_data(data: &WorkflowRecordData) -> Result<(), ActiveValueError> {
    require_nonempty(&data.workflow_type, InputField::Type).map_err(ActiveValueError::Domain)?;
    require_nonempty(&data.state, InputField::State).map_err(ActiveValueError::Domain)?;
    if data.created_at.get() > MAX_EXACT_MILLIS || data.updated_at.get() > MAX_EXACT_MILLIS {
        return Err(ActiveValueError::Domain(DomainError::NowTooLarge));
    }
    if let Some(lease) = &data.lease {
        require_nonempty(&lease.worker, InputField::Worker).map_err(ActiveValueError::Domain)?;
        validate_run_at(lease.deadline).map_err(ActiveValueError::Domain)?;
    }
    if data.version == 0
        || data.latest_version != data.version
        || data.latest_fence != data.fence
        || data.run_state.as_ref().is_some_and(Vec::is_empty)
        || (data.state != b"running" && data.lease.is_some())
        || (terminal_state(&data.state) && data.next_run_at.is_some())
        || data
            .lease
            .as_ref()
            .is_some_and(|lease| data.next_run_at != Some(lease.deadline))
    {
        return Err(ActiveValueError::InvalidExtensions);
    }
    Ok(())
}
