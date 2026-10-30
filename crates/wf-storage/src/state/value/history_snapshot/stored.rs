//! Pure format dispatch; callers supply the actual validated cohort layout.

use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};

use super::super::{
    ScannedWorkflowRecord, StateValueError, ValueSourceFields, binding_source_fields,
    encode_record_fields, encode_record_snapshot, scan_record_point_profiled, scan_record_snapshot,
};
use super::inspect_history_snapshot;

pub(crate) fn scan_record_in_layout<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    admission: AdmissionLimits,
    maximum: usize,
    layout: u16,
) -> Result<ScannedWorkflowRecord<'a>, StateValueError> {
    if layout == 8 && bytes.starts_with(b"FWSR\0\x02") {
        scan_record_snapshot(bytes, scope, partition, id, admission, maximum)
    } else {
        scan_record_point_profiled(bytes, scope, partition, id, admission, maximum)
    }
}

pub(crate) fn encode_record_in_layout(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    admission: AdmissionLimits,
    maximum: usize,
    layout: u16,
) -> Result<Vec<u8>, StateValueError> {
    if layout == 8 {
        encode_record_snapshot(scope, partition, record, ordering, admission, maximum)
    } else {
        encode_record_fields(scope, partition, record, ordering, admission, maximum)
    }
}

pub(crate) fn binding_source_fields_in_layout<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    identity: (&PartitionId, &RecordId),
    history: Option<&[u8]>,
    admission: AdmissionLimits,
    maximum: usize,
    layout: u16,
) -> Result<ValueSourceFields<'a>, StateValueError> {
    if layout != 8 {
        return binding_source_fields(bytes, scope, identity, history, admission, maximum);
    }
    let (partition, id) = identity;
    let Some(event) = history else {
        let scanned =
            scan_record_in_layout(bytes, scope, partition, id, admission, maximum, layout)?;
        return Ok((scanned.version(), Some(scanned.value_fields())));
    };
    if bytes.starts_with(b"FWHS") {
        let scanned = inspect_history_snapshot(bytes, scope, partition, id, admission, maximum)?;
        if scanned.event_id() != event {
            return Err(StateValueError::InvalidValue);
        }
        Ok((scanned.version(), Some(scanned.core().value_fields())))
    } else {
        binding_source_fields(bytes, scope, identity, history, admission, maximum)
    }
}
