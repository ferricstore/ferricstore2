//! Validate scoped minimums and full barrier identity before exposing any record.
use super::budget::{Budget, add, scope_bytes};
use super::{
    StrongWorkflowRecordError as E, StrongWorkflowRecordRequest as R, StrongWorkflowRecordResult,
};
use std::mem::size_of;
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    projection::query_row::{NativeQueryPreparationError as N, NativeQuerySnapshot},
    state::StrongWorkflowRecordRead,
};
use wf_types::identity::PartitionId;

pub(super) fn minimum(request: R<'_>) -> Result<(), E> {
    if request
        .after
        .is_some_and(|p| p.group != request.scope.group() || p.recovery != request.scope.recovery())
    {
        return Err(E::IncompatiblePosition);
    }
    Ok(())
}
pub(super) fn observation(
    source: &StrongWorkflowRecordRead,
    request: R<'_>,
    physical: &PartitionId,
    b: &mut Budget,
) -> Result<(), E> {
    let control = source.snapshot.record.control;
    let applied = control
        .last_entry()
        .map(wf_domain::workflow::control::EntryIdentity::log);
    if let Some(barrier) = source.barrier {
        let current = applied.ok_or(E::Behind)?;
        if current.index() < barrier.index() {
            return Err(E::Behind);
        }
        if current.index() == barrier.index() && current != barrier {
            return Err(E::InvalidObservation);
        }
    }
    if let Some(after) = request.after {
        let applied = applied.ok_or(E::Behind)?;
        if applied.index() < after.index {
            return Err(E::Behind);
        }
    }
    if let Some(capture) = &source.snapshot.record.capture {
        if applied.is_none() {
            return Err(E::InvalidObservation);
        }
        b.spend(add(
            add(
                capture.record.record.id().as_bytes().len(),
                request.id.as_bytes().len(),
            )?,
            add(
                capture.record.partition.as_bytes().len(),
                physical.as_bytes().len(),
            )?,
        )?)?;
        if capture.record.record.id() != request.id || &capture.record.partition != physical {
            return Err(E::InvalidObservation);
        }
    }
    Ok(())
}
pub(super) fn finish(
    native: NativeQuerySnapshot,
    scope: RequestScope,
    request: R<'_>,
    now: wf_types::version::TimestampMillis,
    b: &mut Budget,
) -> Result<StrongWorkflowRecordResult, E> {
    b.peer(native.work_units(), native.temporary_bytes())?;
    let control = native.control();
    let captured_owned = native.owned_bytes();
    if let Some(record) = native.record() {
        b.spend(add(
            add(
                record.logical_partition().map_or(0, <[u8]>::len),
                request.partition.len(),
            )?,
            add(
                record.scope_prefix().map_or(0, <[u8]>::len),
                request.scope_prefix.map_or(0, <[u8]>::len),
            )?,
        )?)?;
        if record.logical_partition() != Some(request.partition)
            || record.scope_prefix() != request.scope_prefix
        {
            return Err(E::InvalidObservation);
        }
    }
    let record = native.into_record().filter(|record| {
        let terminal = matches!(
            record.native_record().record.state(),
            b"completed" | b"failed" | b"cancelled"
        );
        !(terminal
            && record
                .value_binding()
                .lifecycle
                .terminal_deadline
                .is_some_and(|deadline| deadline <= now))
    });
    let mut owned = add(
        size_of::<StrongWorkflowRecordResult>(),
        scope_bytes(&scope)?,
    )?;
    if record.is_some() {
        owned = add(
            owned,
            captured_owned
                .checked_sub(size_of::<NativeQuerySnapshot>())
                .ok_or(E::Limit)?,
        )?;
    }
    b.output(owned)?;
    Ok(StrongWorkflowRecordResult {
        scope,
        control,
        record,
        work: b.work,
        temporary: b.peak,
        owned,
    })
}
pub(super) fn preparation(e: N) -> E {
    match e {
        N::InvalidLimits => E::InvalidLimits,
        N::Limit => E::Limit,
        other => E::Preparation(other),
    }
}
