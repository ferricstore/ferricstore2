//! Coherent control/current capture and bounded exclusive ordered traversal.

use std::{mem::size_of, ops::Bound};

use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    StateTransactionError as Error, StoredWorkflowHistoryRow, WorkflowHistoryDirection,
    WorkflowHistoryPage, WorkflowHistoryQuery,
};
use wf_types::identity::{PartitionId, RecordId};

use super::super::{admission, values::budget::product};
use super::{
    read::{self, HistoryReadContext},
    row,
};
use crate::state::{key, value};

pub(super) fn capture(
    context: &mut HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    query: WorkflowHistoryQuery<'_>,
) -> Result<WorkflowHistoryPage, Error> {
    let bound = context.key_bound(scope, partition, id, query.boundary_event)?;
    // Three independent escaped owners stay live during the range walk.
    context.working = product(bound, 3)?;
    context.temporary(0)?;
    let prefix =
        key::encode_history_prefix(scope, partition, id, bound).map_err(admission::value_error)?;
    let mut upper = context.charge.reserve(prefix.len(), context.live(0)?)?;
    upper.extend_from_slice(&prefix);
    let last = upper.last_mut().ok_or(Error::Corrupt)?;
    *last = last.checked_add(1).ok_or(Error::Corrupt)?;
    let boundary = query
        .boundary_event
        .map(|event| {
            key::encode_history_cursor_key(scope, partition, id, event, bound).map_err(query_error)
        })
        .transpose()?;
    let control = control(context, rows, scope, partition, id)?;
    if query
        .expected_control
        .is_some_and(|expected| expected != control)
    {
        return Err(Error::KnownAborted);
    }
    let (record_version, current) = row::current(context, rows, scope, partition, id)?;
    let current_values = row::materialize(context, partition, id, current)?;
    let mut page = WorkflowHistoryPage {
        control,
        record_version,
        current_values,
        entries: Vec::new(),
        lookahead: None,
        continuation: None,
    };
    let Some(version) = record_version else {
        return Ok(page);
    };
    context.returned(product(query.limit, size_of::<StoredWorkflowHistoryRow>())?)?;
    page.entries = context.charge.reserve(query.limit, context.live(0)?)?;
    let bounds = match (query.direction, boundary.as_deref()) {
        (WorkflowHistoryDirection::Ascending, Some(boundary)) => {
            (Bound::Excluded(boundary), Bound::Excluded(upper.as_slice()))
        }
        (WorkflowHistoryDirection::Descending, Some(boundary)) => (
            Bound::Included(prefix.as_slice()),
            Bound::Excluded(boundary),
        ),
        _ => (
            Bound::Included(prefix.as_slice()),
            Bound::Excluded(upper.as_slice()),
        ),
    };
    context.charge.candidate()?;
    let mut range = rows
        .range::<&[u8]>(bounds)
        .map_err(|e| admission::storage_error(&e))?;
    loop {
        context.charge.output(1, 0)?;
        context.charge.candidate()?;
        let next = match query.direction {
            WorkflowHistoryDirection::Ascending => range.next(),
            WorkflowHistoryDirection::Descending => range.next_back(),
        }
        .transpose()
        .map_err(|e| admission::storage_error(&e))?;
        let Some((key, bytes)) = next else {
            break;
        };
        let live = context.live(0)?;
        read::inspect(&mut context.charge, key.value(), bytes.value(), live)?;
        let entry = row::capture(
            context,
            rows,
            scope,
            partition,
            id,
            (key.value(), bytes.value()),
            (version, current),
        )?;
        if page.entries.len() == query.limit {
            page.lookahead = Some(entry);
            break;
        }
        page.entries.push(entry);
    }
    if page.lookahead.is_some() {
        let last = &page.entries.last().ok_or(Error::Corrupt)?.event.event_id;
        context.charge.work(last.len())?;
        context.returned(last.len())?;
        let bytes = admission::copy_bytes(last.as_bytes())?;
        page.continuation = Some(String::from_utf8(bytes).map_err(|_| Error::Corrupt)?);
    }
    Ok(page)
}

fn control(
    context: &mut HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
) -> Result<ControlState, Error> {
    let bound = context.key_bound(scope, partition, id, None)?;
    let key = key::encode_control_key(scope.group(), scope.recovery(), bound)
        .map_err(admission::value_error)?;
    let live = context.live(key.len())?;
    let raw = read::probe(rows, &mut context.charge, &key, live)?.ok_or(Error::Corrupt)?;
    value::decode_control(
        raw.value(),
        scope.group(),
        scope.recovery(),
        context.limits.maximum_recovery_bytes(),
    )
    .map_err(admission::value_error)
}

fn query_error(error: value::StateValueError) -> Error {
    match error {
        value::StateValueError::Allocation => Error::Allocation,
        value::StateValueError::LengthOverflow | value::StateValueError::Limit { .. } => {
            Error::Limit
        }
        _ => Error::InvalidChanges,
    }
}
