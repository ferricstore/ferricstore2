//! One coherent snapshot/guard validates native companions before metadata copies.
use super::super::{
    budget::{ValueCharge, product, total},
    read,
};
use crate::state::{
    key,
    redb::{
        admission,
        store::{RedbStateStore, RedbStateTransaction},
    },
    value,
};
use redb::ReadableTable;
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::WorkflowValueReadLimits;
use wf_ports::state::StateTransactionError as Error;
use wf_types::identity::{PartitionId, RecordId};

pub(super) fn observe(
    store: &RedbStateStore,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: Option<&str>,
    limits: WorkflowValueReadLimits,
) -> Result<Option<Vec<u8>>, Error> {
    store.ready()?;
    read::check_scope(store, scope)?;
    if limits.maximum_references == 0 {
        return Err(Error::Limit);
    }
    if !matches!(store.layout.get(), 7 | 8) {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let charge = ValueCharge::new(
        limits.maximum_references,
        limits.maximum_stored_bytes,
        limits.maximum_work,
        limits.maximum_temporary_bytes,
    )?;
    let mut context = read::open(store, charge)?;
    if context.binding_format != 2 {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let logical = total(
        read::scope_bytes(scope),
        [
            partition.as_bytes().len(),
            id.as_bytes().len(),
            history.map_or(0, str::len),
        ],
    )?;
    let bound = total(64, [product(logical, 2)?])?;
    let live = total(read::fixed_owner()?, [4096, bound])?;
    context.charge.temporary(live)?;
    context
        .charge
        .work(total(65_536, [product(logical, 32)?])?)?;
    let mut key = key::encode_binding_key(scope, partition, id, history, bound)
        .map_err(admission::value_error)?;
    context.charge.candidate()?;
    let raw = context
        .rows
        .get(key.as_slice())
        .map_err(|e| admission::storage_error(&e))?;
    key[6] = if history.is_some() { 5 } else { 1 };
    context.charge.candidate()?;
    let source = context
        .rows
        .get(key.as_slice())
        .map_err(|e| admission::storage_error(&e))?;
    let (raw, source) = match (raw, source) {
        (None, None) => return Ok(None),
        (Some(raw), Some(source)) => (raw, source),
        _ => return Err(Error::Corrupt),
    };
    context.charge.work(product(
        total(raw.value().len(), [source.value().len()])?,
        4,
    )?)?;
    let data = value::decode_metadata_binding(raw.value(), context.defaults)
        .map_err(admission::value_error)?;
    value::validate_metadata_source(
        &data,
        source.value(),
        scope,
        (partition, id),
        history.map(str::as_bytes),
        context.limits.admission(),
        source.value().len(),
    )
    .map_err(admission::value_error)?;
    context.charge.output(1, data.metadata.len())?;
    context.charge.temporary(total(
        live,
        [data.metadata.len(), std::mem::size_of::<Vec<u8>>()],
    )?)?;
    Ok(Some(admission::copy_bytes(data.metadata)?))
}

pub(crate) fn load_current(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    identity: (&PartitionId, &RecordId),
    incarnation: u128,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<Vec<u8>, Error> {
    charge.helper()?;
    let logical = total(
        read::scope_bytes(scope),
        [identity.0.as_bytes().len(), identity.1.as_bytes().len()],
    )?;
    let bound = total(64, [product(logical, 2)?])?;
    charge.temporary(total(live, [bound, 4096])?)?;
    charge.work(product(logical, 32)?)?;
    let mut key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            identity.0.as_bytes(),
            identity.1.as_bytes(),
        ],
        0,
        |max| key::encode_binding_key(scope, identity.0, identity.1, None, max),
    )?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    charge.candidate()?;
    let raw = table
        .get(key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&key, raw.value())?;
    charge.work(product(raw.value().len(), 4)?)?;
    let data = value::decode_metadata_binding(raw.value(), tx.value_manifest()?.defaults)
        .map_err(admission::value_error)?;
    if data.binding.lifecycle.incarnation != incarnation {
        return Err(Error::Corrupt);
    }
    key[6] = 1;
    tx.step()?;
    charge.candidate()?;
    let source = table
        .get(key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&key, source.value())?;
    charge.work(product(source.value().len(), 4)?)?;
    value::validate_metadata_source(
        &data,
        source.value(),
        scope,
        identity,
        None,
        tx.admission,
        source.value().len(),
    )
    .map_err(admission::value_error)?;
    charge.temporary(total(live, [bound, 4096, data.metadata.len()])?)?;
    admission::copy_bytes(data.metadata)
}
