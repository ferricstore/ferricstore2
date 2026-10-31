//! One bounded exact-prefix range under one coherent guard; no keyspace scan.

use super::super::{
    admission::{copy_bytes, storage_error, table_error, total, value_error},
    store::RedbStateTransaction,
};
use super::{E, require, validate_row};
use crate::state::key;
use redb::ReadableTable;
use std::ops::Bound;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GovernanceCatalogPage, GovernanceCatalogRead};

pub(super) fn key(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    catalog: &[u8],
    member: Option<&[u8]>,
) -> Result<Vec<u8>, E> {
    let fields = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        catalog,
        member.unwrap_or_default(),
    ];
    tx.key(&fields[..if member.is_some() { 5 } else { 4 }], 0, |cap| {
        member.map_or_else(
            || key::governance_catalog::prefix(scope, catalog, cap),
            |member| key::governance_catalog::member(scope, catalog, member, cap),
        )
    })
}
pub(super) fn contains(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
) -> Result<bool, E> {
    tx.step()?;
    let Some(value) = table.get(key).map_err(|e| storage_error(&e))? else {
        return Ok(false);
    };
    tx.recovery_row(key, value.value())?;
    tx.admit_read(total(64, [value.value().len()])?)?;
    validate_row(value.value())?;
    Ok(true)
}
pub(super) fn page(
    tx: &RedbStateTransaction<'_>,
    request: GovernanceCatalogRead<'_>,
) -> Result<GovernanceCatalogPage, E> {
    if !(1..=256).contains(&request.limit) {
        return Err(E::InvalidChanges);
    }
    require(tx, request.scope)?;
    let prefix = key(tx, request.scope, request.catalog, None)?;
    let cursor = request
        .cursor
        .map(|member| key(tx, request.scope, request.catalog, Some(member)))
        .transpose()?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    if let Some(cursor) = &cursor
        && !contains(tx, &table, cursor)?
    {
        return Err(E::CatalogChanged);
    }
    let lower = cursor
        .as_ref()
        .map_or(Bound::Included(prefix.as_slice()), |key| {
            Bound::Excluded(key.as_slice())
        });
    tx.step()?;
    let mut rows = table
        .range::<&[u8]>((lower, Bound::Unbounded))
        .map_err(|e| storage_error(&e))?;
    let count = usize::from(request.limit);
    tx.admit_read(total(
        128,
        [count
            .checked_mul(std::mem::size_of::<Vec<u8>>())
            .ok_or(E::Limit)?],
    )?)?;
    let mut members = Vec::new();
    members
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    for _ in 0..count {
        tx.step()?;
        let Some((physical, value)) = rows.next().transpose().map_err(|e| storage_error(&e))?
        else {
            break;
        };
        if !physical.value().starts_with(&prefix) {
            break;
        }
        tx.recovery_row(physical.value(), value.value())?;
        validate_row(value.value())?;
        let cap = total(256, [physical.value().len()])?;
        tx.temporary(cap)?;
        let decoded = key::decode_state_key(
            physical.value(),
            request.scope.group(),
            request.scope.recovery(),
            physical.value().len(),
            cap,
        )
        .map_err(value_error)?;
        if decoded.family() != 18
            || decoded.part(2) != request.scope.tenant().as_bytes()
            || decoded.part(3) != request.catalog
        {
            return Err(E::Corrupt);
        }
        tx.admit_read(total(24, [decoded.part(4).len()])?)?;
        members.push(copy_bytes(decoded.part(4))?);
    }
    Ok(GovernanceCatalogPage {
        full: members.len() == count,
        members,
    })
}
