//! Scalar companion loading and explicit owned public-reference observations.

use redb::ReadableTable;
use std::mem::size_of;
use wf_domain::workflow::active::{ActiveValueKind, ActiveValueRef, ActiveValueRefs};
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    WorkflowValueBinding, WorkflowValueBindingKnowledge, WorkflowValueReadLimits,
};
use wf_ports::state::{StateTransactionError as Error, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

use super::super::{
    admission,
    store::{RedbStateStore, RedbStateTransaction},
};
use super::{
    budget::{ValueCharge, product, total},
    manifest, read,
};
use crate::state::{
    key,
    value::{self, ValueBindingData},
};

pub(super) fn observe(
    store: &RedbStateStore,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: Option<&str>,
    limits: WorkflowValueReadLimits,
) -> Result<WorkflowValueBindingKnowledge, Error> {
    store.ready()?;
    read::check_scope(store, scope)?;
    if store.layout.get() != 7 {
        return Ok(WorkflowValueBindingKnowledge::LegacyUnavailable);
    }
    if limits.maximum_references == 0 {
        return Err(Error::Limit);
    }
    let charge = ValueCharge::new(
        1,
        limits.maximum_stored_bytes,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    )?;
    let mut context = read::open(store, charge).map_err(|e| store.failed(e))?;
    let logical = total(
        read::scope_bytes(scope),
        [
            partition.as_bytes().len(),
            id.as_bytes().len(),
            history.map_or(0, str::len),
        ],
    )?;
    context.charge.work(total(128, [product(logical, 16)?])?)?;
    let bound = total(64, [product(logical, 2)?])?;
    context
        .charge
        .temporary(total(read::fixed_owner()?, [bound])?)?;
    let key = key::encode_binding_key(scope, partition, id, history, bound)
        .map_err(admission::value_error)?;
    context.charge.candidate()?;
    let found = context
        .rows
        .get(key.as_slice())
        .map_err(|e| store.failed(admission::storage_error(&e)))?;
    let Some(raw) = found else {
        return Ok(WorkflowValueBindingKnowledge::Known(None));
    };
    context.charge.work(total(128, [raw.value().len()])?)?;
    let data = value::decode_binding(raw.value(), context.defaults)
        .map_err(|e| store.failed(admission::value_error(e)))?;
    let live = total(read::fixed_owner()?, [key.len(), raw.value().len()])?;
    let owner = materialize(data, partition, id, &mut context.charge, live)?;
    Ok(WorkflowValueBindingKnowledge::Known(Some(owner)))
}

pub(crate) fn load(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    queries: &[(PartitionId, RecordId)],
    records: &[Option<StoredWorkflowRecord>],
    charge: &mut ValueCharge,
) -> Result<Vec<Option<ValueBindingData>>, Error> {
    charge.helper()?;
    if queries.len() != records.len() {
        return Err(Error::Corrupt);
    }
    charge.work(total(
        128,
        [208, tx.group.as_bytes().len(), tx.recovery.as_bytes().len()],
    )?)?;
    let defaults = manifest::read(tx)?.defaults;
    let fixed = total(
        size_of::<ValueCharge>(),
        [
            size_of::<Vec<Option<ValueBindingData>>>(),
            size_of::<wf_domain::policy::complete::PolicySchemaCharge>(),
        ],
    )?;
    let mut result = charge.reserve(queries.len(), fixed)?;
    let live = total(
        fixed,
        [product(
            queries.len(),
            size_of::<Option<ValueBindingData>>(),
        )?],
    )?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    charge.work(product(queries.len(), 128)?)?;
    for ((partition, id), record) in queries.iter().zip(records) {
        charge.candidate()?;
        let n = total(
            read::scope_bytes(scope),
            [partition.as_bytes().len(), id.as_bytes().len()],
        )?;
        charge.work(product(n, 16)?)?;
        let cap = total(64, [product(n, 2)?])?;
        charge.temporary(total(live, [cap])?)?;
        let key = tx.key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                scope.tenant().as_bytes(),
                partition.as_bytes(),
                id.as_bytes(),
            ],
            0,
            |maximum| key::encode_binding_key(scope, partition, id, None, maximum),
        )?;
        tx.step()?;
        let raw = table
            .get(key.as_slice())
            .map_err(|e| admission::storage_error(&e))?;
        let data = match raw {
            Some(raw) => {
                tx.recovery_row(&key, raw.value())?;
                charge.work(total(128, [raw.value().len()])?)?;
                Some(value::decode_binding(raw.value(), defaults).map_err(admission::value_error)?)
            }
            None => None,
        };
        if record.is_some() != data.is_some() {
            return Err(Error::Corrupt);
        }
        if let (Some(record), Some(data)) = (record, data)
            && data
                .versions
                .iter()
                .flatten()
                .any(|version| *version > record.record.version())
        {
            return Err(Error::Corrupt);
        }
        result.push(data);
    }
    Ok(result)
}

pub(in crate::state::redb) fn materialize(
    data: ValueBindingData,
    partition: &PartitionId,
    id: &RecordId,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<Box<WorkflowValueBinding>, Error> {
    charge.helper()?;
    let count = data.versions.iter().flatten().count();
    let fields = total(partition.as_bytes().len(), [id.as_bytes().len()])?;
    charge.work(product(product(fields, count)?, 2)?)?;
    charge.temporary(total(
        live,
        [
            size_of::<WorkflowValueBinding>(),
            size_of::<Box<WorkflowValueBinding>>(),
            product(count, fields)?,
            if count == 0 {
                0
            } else {
                total(
                    partition.as_bytes().len().max(id.as_bytes().len()),
                    [size_of::<Vec<u8>>()],
                )?
            },
        ],
    )?)?;
    let mut refs = ActiveValueRefs {
        payload: None,
        result: None,
        error: None,
    };
    for (index, target) in [&mut refs.payload, &mut refs.result, &mut refs.error]
        .into_iter()
        .enumerate()
    {
        charge.candidate()?;
        let Some(version) = data.versions[index] else {
            continue;
        };
        let kind = match index {
            0 => ActiveValueKind::Payload,
            1 => ActiveValueKind::Result,
            _ => ActiveValueKind::Error,
        };
        *target = Some(ActiveValueRef::Versioned {
            partition: PartitionId::new(admission::copy_bytes(partition.as_bytes())?)
                .map_err(|_| Error::Corrupt)?,
            id: RecordId::new(admission::copy_bytes(id.as_bytes())?).map_err(|_| Error::Corrupt)?,
            kind,
            version,
        });
    }
    Ok(Box::new(WorkflowValueBinding {
        refs,
        lifecycle: data.lifecycle,
    }))
}
