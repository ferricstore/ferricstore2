//! One cumulative, clock-free validation of complete value and core closure.

use redb::ReadableTable;
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{OriginalValuePartition, OriginalValueRefKind, render_original_value_ref};
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError as Error;
use wf_types::identity::{PartitionId, RecordId};

use super::super::{admission, store::RedbStateTransaction};
use super::{
    budget::{ValueCharge, product, total},
    read,
};
use crate::state::{
    key::{self, DecodedStateKey},
    value::{self, ValueBindingData},
};

mod alias;
use alias::{alias, immutable};

pub(crate) fn cohort(
    tx: &RedbStateTransaction<'_>,
    defaults: WorkflowValueDefaults,
    alias_format: u16,
    layout: u16,
) -> Result<(), Error> {
    let maximum_work = total(
        product(tx.maximum_steps, 65_536)?,
        [product(tx.maximum_wire, 128)?],
    )?;
    let fixed = total(
        std::mem::size_of::<value::ValueSourceFields<'static>>(),
        [
            std::mem::size_of::<Option<&'static [u8]>>(),
            std::mem::size_of::<Option<&'static [Option<&'static [u8]>; 3]>>(),
        ],
    )?;
    tx.temporary(fixed)?;
    let maximum_temporary = tx
        .maximum_temporary
        .checked_sub(fixed)
        .ok_or(Error::Limit)?;
    let mut charge = ValueCharge::new(
        tx.maximum_inspected.max(1),
        tx.maximum_wire.max(1),
        maximum_work,
        maximum_temporary,
    )?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    let mut rows = table.iter().map_err(|e| admission::storage_error(&e))?;
    loop {
        charge.candidate()?;
        tx.step()?;
        let Some(row) = rows.next() else {
            break;
        };
        let (physical, bytes) = row.map_err(|e| admission::storage_error(&e))?;
        if !matches!(physical.value().get(6), Some(1 | 5 | 8..=11)) {
            continue;
        }
        charge.work(total(
            128,
            [product(physical.value().len(), 6)?, bytes.value().len()],
        )?)?;
        tx.recovery_row(physical.value(), bytes.value())?;
        let owner = total(256, [physical.value().len()])?;
        charge.temporary(total(owner, [std::mem::size_of::<ValueCharge>()])?)?;
        tx.temporary(owner)?;
        let decoded = key::decode_state_key(
            physical.value(),
            tx.group,
            tx.recovery,
            physical.value().len(),
            owner,
        )
        .map_err(admission::value_error)?;
        let scope = tx.scope_bytes(decoded.part(2))?;
        match decoded.family() {
            1 | 5 | 10 | 11 => companion(
                tx,
                &table,
                (&decoded, physical.value(), bytes.value()),
                &scope,
                defaults,
                &mut charge,
                (alias_format, layout),
            )?,
            8 => immutable(
                tx,
                &table,
                &scope,
                &decoded,
                bytes.value(),
                &mut charge,
                alias_format,
            )?,
            9 => alias(
                tx,
                &table,
                &scope,
                decoded.part(3),
                bytes.value(),
                &mut charge,
                (None, alias_format),
            )?,
            _ => return Err(Error::Corrupt),
        }
    }
    Ok(())
}

fn companion(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    row: (&DecodedStateKey, &[u8], &[u8]),
    scope: &RequestScope,
    defaults: WorkflowValueDefaults,
    charge: &mut ValueCharge,
    (alias_format, layout): (u16, u16),
) -> Result<(), Error> {
    charge.helper()?;
    charge.work(8_192)?;
    let (decoded, physical, bytes) = row;
    let fields = total(decoded.part(3).len(), [decoded.part(4).len()])?;
    let live = total(
        std::mem::size_of::<ValueCharge>(),
        [physical.len(), fields, 512],
    )?;
    charge.work(fields)?;
    charge.temporary(live)?;
    tx.temporary(fields)?;
    let partition =
        PartitionId::new(admission::copy_bytes(decoded.part(3))?).map_err(|_| Error::Corrupt)?;
    let id = RecordId::new(admission::copy_bytes(decoded.part(4))?).map_err(|_| Error::Corrupt)?;
    charge.work(physical.len())?;
    charge.temporary(total(live, [physical.len()])?)?;
    let mut other = admission::copy_bytes(physical)?;
    other[6] = match decoded.family() {
        1 => 10,
        5 => 11,
        10 => 1,
        11 => 5,
        _ => return Err(Error::Corrupt),
    };
    charge.candidate()?;
    tx.step()?;
    let counterpart = table
        .get(other.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&other, counterpart.value())?;
    charge.work(total(
        128,
        [counterpart.value().len(), product(bytes.len(), 4)?],
    )?)?;
    let (binding, source) = if matches!(decoded.family(), 1 | 5) {
        (counterpart.value(), bytes)
    } else {
        (bytes, counterpart.value())
    };
    let data = value::decode_binding(binding, defaults).map_err(admission::value_error)?;
    let history = matches!(decoded.family(), 5 | 11).then_some(decoded.part(5));
    let maximum_wire = if history.is_some() {
        source.len()
    } else {
        admission::wire_record_limit(tx.admission)?
    };
    if layout == 8 && source.starts_with(b"FWHS") {
        charge.temporary(total(live, [2048, source.len()])?)?;
    }
    let (version, fields) = value::binding_source_fields_in_layout(
        source,
        scope,
        (&partition, &id),
        history,
        tx.admission,
        maximum_wire,
        layout,
    )
    .map_err(admission::value_error)?;
    if data.versions.iter().flatten().any(|value| *value > version) {
        return Err(Error::Corrupt);
    }
    binding_refs(
        tx,
        table,
        scope,
        (&partition, &id),
        data,
        charge,
        (fields.as_ref(), alias_format),
    )?;
    Ok(())
}

fn binding_refs(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    identity: (&PartitionId, &RecordId),
    binding: ValueBindingData,
    charge: &mut ValueCharge,
    (fields, alias_format): (Option<&[Option<&[u8]>; 3]>, u16),
) -> Result<(), Error> {
    charge.helper()?;
    if let Some(fields) = fields {
        value::validate_binding_fields(&binding.versions, fields)
            .map_err(admission::value_error)?;
    }
    for (index, version) in binding.versions.into_iter().enumerate() {
        charge.candidate()?;
        let Some(version) = version else {
            continue;
        };
        let kind = match index {
            0 => OriginalValueRefKind::Payload,
            1 => OriginalValueRefKind::Result,
            _ => OriginalValueRefKind::Error,
        };
        let logical = total(identity.0.as_bytes().len(), [identity.1.as_bytes().len()])?;
        charge.work(product(logical, 4)?)?;
        charge.temporary(total(65_535, [logical, 512])?)?;
        let reference = render_original_value_ref(
            identity.1.as_bytes(),
            kind,
            version,
            OriginalValuePartition::Explicit(identity.0.as_bytes()),
            65_535,
        )
        .map_err(|error| read::build_error(&error))?;
        let maximum = total(
            32,
            [product(
                total(read::scope_bytes(scope), [reference.len()])?,
                2,
            )?],
        )?;
        charge.work(product(
            total(read::scope_bytes(scope), [reference.len()])?,
            16,
        )?)?;
        charge.temporary(total(maximum, [reference.len(), logical, 512])?)?;
        let key = tx.key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                scope.tenant().as_bytes(),
                &reference,
            ],
            0,
            |cap| key::encode_alias_key(scope, &reference, cap),
        )?;
        charge.candidate()?;
        tx.step()?;
        let bytes = table
            .get(key.as_slice())
            .map_err(|e| admission::storage_error(&e))?
            .ok_or(Error::Corrupt)?;
        tx.recovery_row(&key, bytes.value())?;
        charge.work(total(128, [bytes.value().len()])?)?;
        let data = value::decode_alias(bytes.value()).map_err(admission::value_error)?;
        charge.compare(data.partition.len(), identity.0.as_bytes().len())?;
        charge.compare(data.id.len(), identity.1.as_bytes().len())?;
        if data.partition != identity.0.as_bytes()
            || data.id != identity.1.as_bytes()
            || data.kind != kind
            || data.version != version
            || data.incarnation != binding.lifecycle.incarnation
        {
            return Err(Error::Corrupt);
        }
        alias(
            tx,
            table,
            scope,
            &reference,
            bytes.value(),
            charge,
            (fields.and_then(|fields| fields[index]), alias_format),
        )?;
    }
    Ok(())
}
