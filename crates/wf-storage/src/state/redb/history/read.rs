//! One read revision and continuous admission for physical history acquisition.

use std::mem::size_of;

use redb::{ReadableDatabase, TableHandle};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    StateTransactionError as Error, StoredWorkflowHistoryRow, WorkflowHistoryPage,
    WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};
use wf_types::identity::{PartitionId, RecordId};

use super::super::values::{
    budget::{ValueCharge, product, total},
    manifest::{self, ValueManifest},
};
use super::super::{META, ROWS, RedbStateStore, StateStoreLimits, admission};
use crate::state::{key::DecodedOrderedKey, value::ValueBindingData};

pub(super) struct HistoryReadContext {
    metadata: Option<redb::ReadOnlyTable<&'static [u8], &'static [u8]>>,
    read: redb::ReadTransaction,
    pub(super) facts: Option<ValueManifest>,
    pub(super) charge: ValueCharge,
    pub(super) limits: StateStoreLimits,
    pub(super) working: usize,
    returned: usize,
    maximum_returned: usize,
}

impl HistoryReadContext {
    pub(super) fn open_rows(
        &mut self,
    ) -> Result<redb::ReadOnlyTable<&'static [u8], &'static [u8]>, Error> {
        self.charge.candidate()?;
        self.read.open_table(ROWS).map_err(admission::table_error)
    }

    pub(super) fn returned(&mut self, bytes: usize) -> Result<(), Error> {
        let next = total(self.returned, [bytes])?;
        if next > self.maximum_returned {
            return Err(Error::Limit);
        }
        self.charge
            .temporary(total(fixed()?, [self.working, next])?)?;
        self.returned = next;
        Ok(())
    }

    pub(super) fn live(&self, extra: usize) -> Result<usize, Error> {
        total(fixed()?, [self.working, self.returned, extra])
    }

    pub(super) fn temporary(&mut self, extra: usize) -> Result<(), Error> {
        self.charge.temporary(self.live(extra)?)
    }

    pub(super) fn key_bound(
        &mut self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        event: Option<&str>,
    ) -> Result<usize, Error> {
        let bytes = total(
            scope.group().as_bytes().len(),
            [
                scope.recovery().as_bytes().len(),
                scope.tenant().as_bytes().len(),
                partition.as_bytes().len(),
                id.as_bytes().len(),
                event.map_or(0, str::len),
            ],
        )?;
        self.charge.work(total(128, [product(bytes, 16)?])?)?;
        let bound = total(80, [product(bytes, 2)?])?;
        self.temporary(bound)?;
        Ok(bound)
    }
}

fn fixed() -> Result<usize, Error> {
    total(
        size_of::<HistoryReadContext>(),
        [
            size_of::<redb::ReadOnlyTable<&[u8], &[u8]>>(),
            size_of::<redb::Range<'_, &[u8], &[u8]>>(),
            product(3, size_of::<redb::AccessGuard<'_, &[u8]>>())?,
            product(4, size_of::<Vec<u8>>())?,
            product(2, size_of::<ValueBindingData>())?,
            size_of::<DecodedOrderedKey>(),
            size_of::<StoredWorkflowHistoryRow>(),
            META.name().len(),
            ROWS.name().len(),
        ],
    )
}

pub(super) fn open(
    store: &RedbStateStore,
    scope: &RequestScope,
    query: WorkflowHistoryQuery<'_>,
    limits: WorkflowHistoryReadLimits,
) -> Result<HistoryReadContext, Error> {
    store.ready()?;
    if scope.group() != &store.group || scope.recovery() != &store.recovery {
        return Err(Error::InvalidChanges);
    }
    if query.limit == 0 {
        return Err(Error::InvalidChanges);
    }
    if query.limit > limits.maximum_entries || limits.maximum_returned_bytes == 0 {
        return Err(Error::Limit);
    }
    total(query.limit, [1])?;
    product(query.limit, size_of::<StoredWorkflowHistoryRow>())?;
    let mut charge = ValueCharge::new(
        limits.maximum_row_reads,
        limits.maximum_inspected_bytes,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    )?;
    charge.temporary(total(fixed()?, [size_of::<WorkflowHistoryPage>()])?)?;
    if size_of::<WorkflowHistoryPage>() > limits.maximum_returned_bytes {
        return Err(Error::Limit);
    }
    charge.candidate()?;
    let read = store
        .database
        .begin_read()
        .map_err(admission::transaction_error)?;
    let mut context = HistoryReadContext {
        metadata: None,
        read,
        facts: None,
        charge,
        limits: store.limits,
        working: 0,
        returned: size_of::<WorkflowHistoryPage>(),
        maximum_returned: limits.maximum_returned_bytes,
    };
    if matches!(store.layout.get(), 7 | 8) {
        load_manifest(&mut context, store)?;
    }
    Ok(context)
}

fn load_manifest(context: &mut HistoryReadContext, store: &RedbStateStore) -> Result<(), Error> {
    context.charge.candidate()?;
    let table = context
        .read
        .open_table(META)
        .map_err(admission::table_error)?;
    let live = context.live(0)?;
    let raw = probe(&table, &mut context.charge, b"manifest", live)?.ok_or(Error::Corrupt)?;
    context.facts = Some(manifest::parse(raw.value(), &store.group, &store.recovery)?);
    drop(raw);
    context.metadata = Some(table);
    Ok(())
}

pub(super) fn inspect(
    charge: &mut ValueCharge,
    key: &[u8],
    bytes: &[u8],
    live: usize,
) -> Result<(), Error> {
    let physical = total(key.len(), [bytes.len()])?;
    charge.output(0, physical)?;
    charge.work(total(128, [product(physical, 16)?])?)?;
    charge.temporary(total(live, [physical])?)
}

pub(super) fn probe<'table>(
    table: &'table redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    key: &[u8],
    live: usize,
) -> Result<Option<redb::AccessGuard<'table, &'static [u8]>>, Error> {
    charge.output(1, 0)?;
    charge.candidate()?;
    let found = table.get(key).map_err(|e| admission::storage_error(&e))?;
    inspect(
        charge,
        key,
        found.as_ref().map_or(&[], redb::AccessGuard::value),
        live,
    )?;
    Ok(found)
}

pub(super) fn close(
    context: HistoryReadContext,
    rows: redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
) {
    drop(rows);
    drop(context.metadata);
    drop(context.read);
}
