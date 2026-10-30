//! Shared nested emitters write into one prospectively admitted envelope owner.

use wf_domain::history::HistoryEntry;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::PartitionId;

use super::super::{StateValueError, add, allocate, history, record};
use super::scan::BorrowedHistoryFields;
use super::{HistorySnapshotLimits, admit};

pub(super) fn encode(
    scope: &RequestScope,
    postimage: &StoredWorkflowRecord,
    event: &HistoryEntry,
    limits: HistorySnapshotLimits,
) -> Result<Vec<u8>, StateValueError> {
    if postimage.tenant != *scope.tenant() {
        return Err(StateValueError::InvalidValue);
    }
    encode_fields(
        scope,
        &postimage.partition,
        &postimage.record,
        postimage.ordering,
        event,
        limits,
    )
}

pub(super) fn measure(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    event: &HistoryEntry,
    limits: HistorySnapshotLimits,
) -> Result<(usize, usize, usize), StateValueError> {
    let (kind, version, fence) = record.latest_event_witness();
    BorrowedHistoryFields::from_event(event).validate(
        &record::RecordHistoryView {
            state: record.state(),
            version: record.version(),
            fence: record.fence().get(),
            attempts: record.attempts(),
            updated: record.updated_at().get(),
            next: record
                .next_run_at()
                .map(wf_types::version::TimestampMillis::get),
            worker: record.lease().map(|lease| lease.worker.as_slice()),
            deadline: record.lease().map_or(0, |lease| lease.deadline.get()),
            latest: (kind, version, fence.get()),
        },
        limits.admission,
    )?;
    let namespace = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        partition.as_bytes(),
    ];
    let record_size = record::measure_fields(
        namespace,
        record,
        ordering,
        limits.admission,
        limits.encoded,
    )?;
    let history_size = history::history_size(scope, partition, record.id(), event)?;
    u32::try_from(record_size).map_err(|_| StateValueError::LengthOverflow)?;
    u32::try_from(history_size).map_err(|_| StateValueError::LengthOverflow)?;
    let mut encoded = 14;
    add(&mut encoded, record_size)?;
    add(&mut encoded, history_size)?;
    admit(encoded, limits.encoded)?;
    let mut workspace = encoded;
    add(&mut workspace, 2048)?;
    admit(workspace, limits.workspace)?;
    Ok((record_size, history_size, encoded))
}

pub(super) fn encode_fields(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    event: &HistoryEntry,
    limits: HistorySnapshotLimits,
) -> Result<Vec<u8>, StateValueError> {
    let (record_size, history_size, encoded) =
        measure(scope, partition, record, ordering, event, limits)?;
    let record_length = u32::try_from(record_size).map_err(|_| StateValueError::LengthOverflow)?;
    let history_length =
        u32::try_from(history_size).map_err(|_| StateValueError::LengthOverflow)?;
    let namespace = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        partition.as_bytes(),
    ];
    let mut output = allocate(encoded, limits.encoded, *b"FWHS")?;
    output.extend_from_slice(&record_length.to_be_bytes());
    record::write_fields(&mut output, namespace, record, ordering, 2)?;
    output.extend_from_slice(&history_length.to_be_bytes());
    output.extend_from_slice(b"FWSH\0\x02");
    history::put_history_body(
        &mut output,
        scope,
        partition,
        record.id(),
        event,
        record::event_tag_version(event.kind, 2)?,
    )?;
    if output.len() != encoded {
        return Err(StateValueError::InvalidValue);
    }
    Ok(output)
}
