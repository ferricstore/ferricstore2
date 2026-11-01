//! Metadata binding output shares the already consumed core writer.
use super::super::{
    budget::{ValueCharge, product, total},
    read::scope_bytes,
};
use crate::state::{
    key,
    redb::{admission, publication, store::RedbStateTransaction},
    value,
};
use redb::ReadableTable;
use sha2::{Digest, Sha256};
use wf_domain::workflow::batch::ScopedPlan;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError as Error;

pub(crate) fn publish(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    scoped: &ScopedPlan,
    (binding, supplied): (value::ValueBindingData, Option<&[u8]>),
    current: bool,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.helper()?;
    let record = &scoped.plan().write().ok_or(Error::InvalidChanges)?.record;
    let event = scoped.plan().history().ok_or(Error::InvalidChanges)?;
    let history = (!current).then_some(event.event_id.as_str());
    let logical = total(
        scope_bytes(scope),
        [
            scoped.partition().as_bytes().len(),
            record.id().as_bytes().len(),
            history.map_or(0, str::len),
        ],
    )?;
    let bound = total(64, [product(logical, 2)?])?;
    charge.work(product(logical, 48)?)?;
    charge.temporary(total(live, [product(bound, 3)?, 4096])?)?;
    let binding_key = make_key(tx, scope, scoped, history)?;
    tx.temporary(binding_key.len())?;
    let mut source_key = admission::copy_bytes(&binding_key)?;
    source_key[6] = if current { 1 } else { 5 };
    let current_key = if supplied.is_none() {
        Some(make_key(tx, scope, scoped, None)?)
    } else {
        None
    };
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    charge.candidate()?;
    let source = table
        .get(source_key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&source_key, source.value())?;
    charge.work(product(source.value().len(), 4)?)?;
    let fallback = if let Some(key) = &current_key {
        tx.step()?;
        charge.candidate()?;
        let found = table
            .get(key.as_slice())
            .map_err(|e| admission::storage_error(&e))?
            .ok_or(Error::Corrupt)?;
        tx.recovery_row(key, found.value())?;
        charge.work(product(found.value().len(), 4)?)?;
        Some(found)
    } else {
        None
    };
    let decoded = fallback
        .as_ref()
        .map(|raw| {
            value::decode_metadata_binding(raw.value(), tx.value_manifest()?.defaults)
                .map_err(admission::value_error)
        })
        .transpose()?;
    if decoded
        .as_ref()
        .is_some_and(|old| old.binding.lifecycle.incarnation != binding.lifecycle.incarnation)
    {
        return Err(Error::Corrupt);
    }
    let metadata = supplied
        .or(decoded.as_ref().map(|old| old.metadata))
        .ok_or(Error::InvalidChanges)?;
    let core =
        value::metadata_source_core(source.value(), !current).map_err(admission::value_error)?;
    let data = value::RecordMetadataBinding {
        binding,
        version: record.version(),
        core_digest: Sha256::digest(core).into(),
        metadata,
    };
    let size = value::encoded_metadata_binding_size(&data).map_err(admission::value_error)?;
    charge.output(1, total(binding_key.len(), [size])?)?;
    charge.work(product(size, 4)?)?;
    charge.temporary(total(live, [product(bound, 3)?, size, 197, 4096])?)?;
    tx.temporary(total(binding_key.len(), [size, 197, 4096])?)?;
    admission::engine_pair(binding_key.len(), size)?;
    let encoded = value::encode_metadata_binding(&data, size).map_err(admission::value_error)?;
    drop(fallback);
    drop(source);
    drop(table);
    write_output(tx, &binding_key, &encoded, current, charge)
}

fn make_key(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    scoped: &ScopedPlan,
    history: Option<&str>,
) -> Result<Vec<u8>, Error> {
    let record = &scoped.plan().write().ok_or(Error::InvalidChanges)?.record;
    tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            scoped.partition().as_bytes(),
            record.id().as_bytes(),
            history.unwrap_or("").as_bytes(),
        ],
        8,
        |max| key::encode_binding_key(scope, scoped.partition(), record.id(), history, max),
    )
}

fn write_output(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    bytes: &[u8],
    current: bool,
    charge: &mut ValueCharge,
) -> Result<(), Error> {
    if !current {
        tx.step()?;
        let table = tx
            .write
            .open_table(tx.rows_table)
            .map_err(admission::table_error)?;
        tx.step()?;
        charge.candidate()?;
        if let Some(old) = table.get(key).map_err(|e| admission::storage_error(&e))? {
            charge.compare(old.value().len(), bytes.len())?;
            if old.value() != bytes {
                return Err(Error::Corrupt);
            }
            return Ok(());
        }
    }
    publication::put(tx, key, bytes)
}
