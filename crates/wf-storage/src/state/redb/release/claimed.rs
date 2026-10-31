//! Consumed paid-ID custody; durable Claim publication is its only future creator.
use super::{
    super::{
        admission::{storage_error, table_error, value_error},
        store::RedbStateTransaction,
    },
    read,
};
use crate::state::{
    key::{self, DecodedStateKey},
    value,
};
use redb::ReadableTable;
use wf_domain::{
    governance::identified::{CreditReservationStatus as S, IdentifiedCreditPlan},
    workflow::request::RequestScope,
};
use wf_ports::{log::ReleaseBindingRef, state::StateTransactionError as E};

pub(super) fn reservation_key(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    credit_scope: &[u8],
    shard: u64,
    epoch: u64,
    id: &[u8],
) -> Result<Vec<u8>, E> {
    tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
            &shard.to_be_bytes(),
            &epoch.to_be_bytes(),
            id,
        ],
        0,
        |cap| key::encode_credit_reservation_key(scope, credit_scope, shard, epoch, id, cap),
    )
}

pub(super) fn active(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
    id: &[u8],
) -> Result<bool, E> {
    Ok(read::read(tx, table, key, |bytes| {
        let row = value::credit::reservation(bytes).map_err(value_error)?;
        if row.id != id {
            return Err(E::Corrupt);
        }
        Ok(row.status == S::Active)
    })?
    .unwrap_or(false))
}

pub(in crate::state::redb) fn row(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
) -> Result<(), E> {
    marker(bytes)?;
    let scope = tx.scope_bytes(key.part(2))?;
    let shard = u64::from_be_bytes(key.part(4).try_into().map_err(|_| E::Corrupt)?);
    let epoch = u64::from_be_bytes(key.part(5).try_into().map_err(|_| E::Corrupt)?);
    let reservation = reservation_key(tx, &scope, key.part(3), shard, epoch, key.part(6))?;
    if !active(tx, table, &reservation, key.part(6))? {
        return Err(E::Corrupt);
    }
    Ok(())
}

pub(super) fn check_binding(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    binding: ReleaseBindingRef<'_>,
) -> Result<(), E> {
    let intent = binding.intent;
    let mut key = reservation_key(
        tx,
        scope,
        intent.scope,
        intent.shard_id,
        binding.epoch,
        intent.reservation_id,
    )?;
    if active(tx, table, &key, intent.reservation_id)? {
        key[6] = 24;
        read::read(tx, table, &key, marker)?.ok_or(E::Corrupt)?;
    }
    Ok(())
}

pub(super) fn marker(bytes: &[u8]) -> Result<(), E> {
    if bytes != b"FWGU\0\x01" {
        return Err(E::Corrupt);
    }
    Ok(())
}

pub(in crate::state::redb) fn retire(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    plan: &IdentifiedCreditPlan<'_>,
) -> Result<(), E> {
    if tx.catalog_profile != 5 {
        return Ok(());
    }
    tx.require_governed_claim()?;
    for change in plan
        .reservations()
        .iter()
        .filter(|change| change.status != Some(S::Active))
    {
        let mut key = reservation_key(
            tx,
            scope,
            plan.scope(),
            change.shard_id,
            change.epoch,
            &change.id,
        )?;
        key[6] = 24;
        tx.step()?;
        let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        if read::read(tx, &table, &key, marker)?.is_some() {
            tx.step()?;
            table
                .remove(key.as_slice())
                .map_err(|e| storage_error(&e))?;
        }
    }
    Ok(())
}
