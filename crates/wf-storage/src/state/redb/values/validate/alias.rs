//! Alias identity, immutable closure and representation-aware source checks.

use sha2::{Digest, Sha256};

use super::{
    DecodedStateKey, Error, OriginalValuePartition, ReadableTable, RedbStateTransaction,
    RequestScope, ValueCharge, admission, key, product, read, render_original_value_ref, total,
    value,
};

pub(super) fn alias(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    reference: &[u8],
    bytes: &[u8],
    charge: &mut ValueCharge,
    (expected, alias_format): (Option<&[u8]>, u16),
) -> Result<(), Error> {
    charge.helper()?;
    charge.work(bytes.len())?;
    let alias = value::decode_alias(bytes).map_err(admission::value_error)?;
    if alias.stored && alias_format != 2 {
        return Err(Error::Corrupt);
    }
    charge.work(product(total(alias.partition.len(), [alias.id.len()])?, 4)?)?;
    let maximum = total(
        64,
        [product(
            total(read::scope_bytes(scope), [reference.len()])?,
            2,
        )?],
    )?;
    charge.temporary(total(maximum, [65_535, bytes.len(), 512])?)?;
    let rendered = render_original_value_ref(
        alias.id,
        alias.kind,
        alias.version,
        OriginalValuePartition::Explicit(alias.partition),
        65_535,
    )
    .map_err(|_| Error::Corrupt)?;
    charge.compare(rendered.len(), reference.len())?;
    if rendered != reference {
        return Err(Error::Corrupt);
    }
    charge.work(product(
        total(read::scope_bytes(scope), [reference.len()])?,
        16,
    )?)?;
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            reference,
        ],
        16,
        |cap| key::encode_immutable_key(scope, reference, alias.incarnation, cap),
    )?;
    charge.candidate()?;
    tx.step()?;
    let stored = table
        .get(key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&key, stored.value())?;
    charge.work(total(128, [stored.value().len()])?)?;
    if alias.stored_len != u64::try_from(stored.value().len()).map_err(|_| Error::Limit)?
        || Sha256::digest(stored.value()).as_slice() != alias.digest
    {
        return Err(Error::Corrupt);
    }
    if alias.stored {
        if let Some(expected) = expected {
            charge.compare(stored.value().len(), expected.len())?;
            if stored.value() != expected {
                return Err(Error::Corrupt);
            }
        }
        return Ok(());
    }
    if let Some(expected) = expected {
        charge.compare(stored.value().len(), total(expected.len(), [5])?)?;
    }
    value::validate_stored_binary(stored.value(), expected).map_err(admission::value_error)
}

pub(super) fn immutable(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    decoded: &DecodedStateKey,
    bytes: &[u8],
    charge: &mut ValueCharge,
    alias_format: u16,
) -> Result<(), Error> {
    charge.helper()?;
    let reference = decoded.part(3);
    charge.work(product(
        total(read::scope_bytes(scope), [reference.len()])?,
        16,
    )?)?;
    let maximum = total(
        32,
        [product(
            total(read::scope_bytes(scope), [reference.len()])?,
            2,
        )?],
    )?;
    charge.temporary(total(
        product(maximum, 2)?,
        [256, std::mem::size_of::<ValueCharge>()],
    )?)?;
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            reference,
        ],
        0,
        |cap| key::encode_alias_key(scope, reference, cap),
    )?;
    charge.candidate()?;
    tx.step()?;
    let raw = table
        .get(key.as_slice())
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(&key, raw.value())?;
    charge.work(total(128, [raw.value().len(), bytes.len()])?)?;
    let data = value::decode_alias(raw.value()).map_err(admission::value_error)?;
    if data.incarnation != decoded.number()
        || data.stored_len != u64::try_from(bytes.len()).map_err(|_| Error::Limit)?
        || Sha256::digest(bytes).as_slice() != data.digest
    {
        return Err(Error::Corrupt);
    }
    alias(
        tx,
        table,
        scope,
        reference,
        raw.value(),
        charge,
        (None, alias_format),
    )
}
