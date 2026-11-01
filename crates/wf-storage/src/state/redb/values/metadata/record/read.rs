//! One pinned redb read transaction and cumulative physical/prospective accounts.

use redb::ReadableDatabase;
use std::mem::size_of;
use wf_domain::workflow::{control::ControlState, request::RequestScope};
use wf_ports::state::{
    StateTransactionError as Error, StoredWorkflowRecordCapture, WorkflowRecordReadLimits,
    WorkflowRecordReadQuery, WorkflowRecordReadSnapshot,
};
use wf_types::identity::{PartitionId, RecordId};

use crate::state::redb::values::{
    budget::{ValueCharge, product, total},
    manifest::{self, ValueManifest},
};
use crate::state::redb::{META, ROWS, RedbStateStore, StateStoreLimits, admission};
use crate::state::{key, value};

pub(super) struct RecordReadContext {
    pub(super) rows: redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    metadata: redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    read: redb::ReadTransaction,
    pub(super) facts: ValueManifest,
    pub(super) charge: ValueCharge,
    pub(super) store_limits: StateStoreLimits,
    limits: WorkflowRecordReadLimits,
    pub(super) key_bound: usize,
    returned: usize,
}

pub(super) fn observe(
    store: &RedbStateStore,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    query: WorkflowRecordReadQuery<'_>,
    limits: WorkflowRecordReadLimits,
) -> Result<WorkflowRecordReadSnapshot, Error> {
    let (context, control, capture) = begin(store, scope, partition, id, query, limits)?;
    Ok(finish(context, control, capture))
}

pub(super) fn begin(
    store: &RedbStateStore,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    query: WorkflowRecordReadQuery<'_>,
    limits: WorkflowRecordReadLimits,
) -> Result<
    (
        RecordReadContext,
        ControlState,
        Option<StoredWorkflowRecordCapture>,
    ),
    Error,
> {
    let mut context = open(store, scope, partition, id, query, limits)?;
    let control = context.control(scope)?;
    if query
        .expected_control
        .is_some_and(|expected| expected != control)
    {
        return Err(Error::KnownAborted);
    }
    let capture = super::row::capture(&mut context, scope, partition, id, query.history)?;
    Ok((context, control, capture))
}

pub(super) fn finish(
    context: RecordReadContext,
    control: ControlState,
    capture: Option<StoredWorkflowRecordCapture>,
) -> WorkflowRecordReadSnapshot {
    let (row_reads, inspected_bytes, work_units) = context.charge.usage();
    let snapshot = WorkflowRecordReadSnapshot {
        control,
        capture,
        row_reads,
        inspected_bytes,
        returned_bytes: context.returned,
        work_units,
        temporary_bytes: context.charge.peak(),
    };
    drop(context.rows);
    drop(context.metadata);
    drop(context.read);
    snapshot
}

pub(super) fn open(
    store: &RedbStateStore,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    query: WorkflowRecordReadQuery<'_>,
    limits: WorkflowRecordReadLimits,
) -> Result<RecordReadContext, Error> {
    store.ready()?;
    super::super::super::read::check_scope(store, scope)?;
    let mut charge = ValueCharge::new(
        limits.maximum_row_reads,
        limits.maximum_inspected_bytes,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    )?;
    if limits.maximum_returned_bytes == 0 {
        return Err(Error::Limit);
    }
    let logical = total(
        super::super::super::read::scope_bytes(scope),
        [
            partition.as_bytes().len(),
            id.as_bytes().len(),
            query.history.map_or(0, str::len),
        ],
    )?;
    let key_bound = total(80, [product(logical, 2)?])?;
    charge.work(product(logical, 32)?)?;
    charge.temporary(total(fixed()?, [key_bound])?)?;
    if let Some(event) = query.history {
        value::canonical_history_position(event.as_bytes()).map_err(query_error)?;
    }
    if store.layout.get() != 8 {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let read = store
        .database
        .begin_read()
        .map_err(admission::transaction_error)?;
    let rows = read.open_table(ROWS).map_err(admission::table_error)?;
    let metadata = read.open_table(META).map_err(admission::table_error)?;
    let raw = probe(&metadata, &mut charge, b"manifest", fixed()?)?.ok_or(Error::Corrupt)?;
    let facts = manifest::parse(raw.value(), &store.group, &store.recovery)?;
    if facts.layout != 8 || facts.binding_format != 2 || facts.alias_format != 2 {
        return Err(Error::UnsupportedPolicyProfile);
    }
    drop(raw);
    let mut context = RecordReadContext {
        rows,
        metadata,
        read,
        facts,
        charge,
        store_limits: store.limits,
        limits,
        key_bound,
        returned: 0,
    };
    context.returned(size_of::<WorkflowRecordReadSnapshot>())?;
    Ok(context)
}

impl RecordReadContext {
    pub(super) fn control(&mut self, scope: &RequestScope) -> Result<ControlState, Error> {
        self.charge.temporary(self.live(self.key_bound)?)?;
        let key = key::encode_control_key(scope.group(), scope.recovery(), self.key_bound)
            .map_err(admission::value_error)?;
        let live = self.live(key.len())?;
        let raw = probe(&self.rows, &mut self.charge, &key, live)?.ok_or(Error::Corrupt)?;
        value::decode_control(
            raw.value(),
            scope.group(),
            scope.recovery(),
            self.store_limits.maximum_recovery_bytes(),
        )
        .map_err(admission::value_error)
    }

    pub(super) fn live(&self, extra: usize) -> Result<usize, Error> {
        total(fixed()?, [self.returned, extra])
    }

    pub(super) fn returned(&mut self, bytes: usize) -> Result<(), Error> {
        let next = self.next_returned(bytes)?;
        self.charge.temporary(total(fixed()?, [next])?)?;
        self.returned = next;
        Ok(())
    }

    pub(super) fn next_returned(&self, bytes: usize) -> Result<usize, Error> {
        let next = total(self.returned, [bytes])?;
        if next > self.limits.maximum_returned_bytes {
            return Err(Error::Limit);
        }
        Ok(next)
    }
}

pub(super) fn probe<'table>(
    table: &'table redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    key: &[u8],
    live: usize,
) -> Result<Option<redb::AccessGuard<'table, &'static [u8]>>, Error> {
    charge.output(1, key.len())?;
    charge.candidate()?;
    charge.work(key.len())?;
    let raw = table.get(key).map_err(|e| admission::storage_error(&e))?;
    let bytes = raw.as_ref().map_or(0, |raw| raw.value().len());
    charge.output(0, bytes)?;
    charge.work(total(128, [product(bytes, 16)?])?)?;
    charge.temporary(total(live, [key.len(), bytes])?)?;
    Ok(raw)
}

fn fixed() -> Result<usize, Error> {
    // Fixed control covers scanners, digest state, guards, scalar bindings and
    // helper stack; physical guard backing and final owners are admitted apart.
    total(4096, [size_of::<RecordReadContext>()])
}

pub(super) fn query_error(error: value::StateValueError) -> Error {
    match error {
        value::StateValueError::Allocation => Error::Allocation,
        value::StateValueError::LengthOverflow | value::StateValueError::Limit { .. } => {
            Error::Limit
        }
        _ => Error::InvalidChanges,
    }
}
