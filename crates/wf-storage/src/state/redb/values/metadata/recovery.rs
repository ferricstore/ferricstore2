//! Shared manifest-format and exact native companion validation.
use super::super::budget::{ValueCharge, product, total};
use crate::state::{
    redb::{admission, store::RedbStateTransaction},
    value,
};
use redb::ReadableTable;
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError as Error;
use wf_types::identity::{PartitionId, RecordId};

pub(crate) fn current(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    (key, bytes): (&[u8], &[u8]),
    (scope, partition, id): (&RequestScope, &PartitionId, &RecordId),
    (defaults, format): (WorkflowValueDefaults, u16),
    (charge, live): (&mut ValueCharge, usize),
) -> Result<value::ValueBindingData, Error> {
    if format == 1 {
        if !bytes.starts_with(b"FWVB\0\x01") {
            return Err(Error::Corrupt);
        }
        return value::decode_binding(bytes, defaults).map_err(admission::value_error);
    }
    if format != 2 {
        return Err(Error::Corrupt);
    }
    charge.work(product(bytes.len(), 4)?)?;
    let data = value::decode_metadata_binding(bytes, defaults).map_err(admission::value_error)?;
    charge.temporary(total(live, [key.len(), 4096])?)?;
    tx.temporary(total(key.len(), [4096])?)?;
    let mut core_key = admission::copy_bytes(key)?;
    core_key[6] = 1;
    tx.step()?;
    charge.candidate()?;
    let source = table
        .get(core_key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&core_key, source.value())?;
    charge.work(product(source.value().len(), 4)?)?;
    value::validate_metadata_source(
        &data,
        source.value(),
        scope,
        (partition, id),
        None,
        tx.admission,
        source.value().len(),
    )
    .map_err(admission::value_error)?;
    Ok(data.binding)
}

pub(crate) fn companion(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    identity: (&PartitionId, &RecordId),
    history: Option<&[u8]>,
    bytes: (&[u8], &[u8]),
    profile: (WorkflowValueDefaults, u16),
) -> Result<(), Error> {
    let (binding, source) = bytes;
    if profile.1 == 1 {
        if !binding.starts_with(b"FWVB\0\x01") {
            return Err(Error::Corrupt);
        }
        return Ok(());
    }
    if profile.1 != 2 {
        return Err(Error::Corrupt);
    }
    let data =
        value::decode_metadata_binding(binding, profile.0).map_err(admission::value_error)?;
    value::validate_metadata_source(
        &data,
        source,
        scope,
        identity,
        history,
        tx.admission,
        source.len(),
    )
    .map_err(admission::value_error)
}
