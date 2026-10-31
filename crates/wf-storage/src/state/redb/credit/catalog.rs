//! Same-writer publication append and coherent bounded queue observations.

use super::super::{
    admission::{copy_bytes, storage_error, table_error, total, value_error},
    publication,
    store::RedbStateTransaction,
};
use super::CreditReadContext;
use crate::state::{key, value};
use redb::ReadableTable;
use wf_domain::governance::{
    catalog::{CreditCatalogMeta, CreditCatalogObservation},
    identified::IdentifiedCreditPlan,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{CreditCatalogEntry, CreditCatalogPage, StateTransactionError as E};

pub(super) mod ack;
pub(in crate::state::redb) mod recovery;

pub(crate) fn observation(
    context: &CreditReadContext<'_, '_>,
) -> Result<(CreditCatalogMeta, Option<Vec<u8>>), E> {
    let tx = context.tx;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    let meta = read_meta(tx, &table, context.scope)?.unwrap_or(CreditCatalogMeta::empty());
    let tail = if meta.head() <= meta.tail() {
        let key = catalog_key(tx, context.scope, Some(meta.tail()))?;
        Some(
            read(tx, &table, &key, |bytes| {
                let scope = value::catalog::intent(bytes).map_err(value_error)?;
                tx.admit_read(total(scope.len(), [24])?)?;
                copy_bytes(scope)
            })?
            .ok_or(E::Corrupt)?,
        )
    } else {
        None
    };
    Ok((meta, tail))
}

pub(super) fn publish(
    context: &CreditReadContext<'_, '_>,
    plan: &IdentifiedCreditPlan<'_>,
) -> Result<(), E> {
    if !context.catalog {
        use wf_domain::governance::identified::{
            IdentifiedCreditDecision as D, IdentifiedCreditRejection as R,
        };
        return if plan.catalog_publication().is_none()
            && !matches!(
                plan.decision(),
                D::Rejected(R::CatalogBacklogFull | R::CatalogSequenceExhausted)
            ) {
            Ok(())
        } else {
            Err(E::InvalidChanges)
        };
    }
    let exists = context.read(&context.owner_key()?, |_| Ok(()))?.is_some();
    if exists || plan.changed_owner().is_none() {
        return if plan.catalog_publication().is_none() {
            Ok(())
        } else {
            Err(E::InvalidChanges)
        };
    }
    let (meta, tail) = observation(context)?;
    let expected = CreditCatalogObservation {
        meta,
        tail_scope: tail.as_deref(),
    }
    .publication(plan.scope(), 65_535)
    .map_err(|_| E::InvalidChanges)?;
    if expected != plan.catalog_publication() {
        return Err(E::InvalidChanges);
    }
    let Some(next) = expected else {
        return Ok(());
    };
    let tx = context.tx;
    let key = catalog_key(tx, context.scope, Some(next.tail()))?;
    let old = context.read(&key, |bytes| {
        let scope = value::catalog::intent(bytes).map_err(value_error)?;
        tx.admit_read(total(scope.len(), [24])?)?;
        copy_bytes(scope)
    })?;
    if let Some(old) = old
        && old != plan.scope()
        && owner_exists(tx, context.scope, &old)?
    {
        return Err(E::Corrupt);
    }
    tx.temporary(total(32, [plan.scope().len()])?)?;
    let bytes = value::catalog::encode_intent(plan.scope()).map_err(value_error)?;
    publication::put(tx, &key, &bytes)?;
    let key = catalog_key(tx, context.scope, None)?;
    tx.temporary(22)?;
    publication::put(tx, &key, &value::catalog::encode_meta(next))
}

pub(super) fn page(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    limit: u16,
) -> Result<CreditCatalogPage, E> {
    if !(1..=256).contains(&limit) {
        return Err(E::InvalidChanges);
    }
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    tx.admit_read(512)?;
    let manifest = tx.value_manifest()?;
    if !manifest.credit_catalog {
        return Err(E::UnsupportedPolicyProfile);
    }
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    let meta = read_meta(tx, &table, scope)?.unwrap_or(CreditCatalogMeta::empty());
    let count = usize::try_from((meta.tail() + 1 - meta.head()).min(u64::from(limit)))
        .map_err(|_| E::Limit)?;
    tx.admit_read(total(
        128,
        [count
            .checked_mul(std::mem::size_of::<CreditCatalogEntry>())
            .ok_or(E::Limit)?],
    )?)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    for offset in 0..count {
        let sequence = meta.head() + u64::try_from(offset).map_err(|_| E::Limit)?;
        let key = catalog_key(tx, scope, Some(sequence))?;
        let credit_scope = read(tx, &table, &key, |bytes| {
            let scope = value::catalog::intent(bytes).map_err(value_error)?;
            tx.admit_read(scope.len())?;
            copy_bytes(scope)
        })?
        .ok_or(E::Corrupt)?;
        entries.push(CreditCatalogEntry {
            sequence,
            credit_scope,
        });
    }
    Ok(CreditCatalogPage {
        meta,
        more: u64::try_from(count).map_err(|_| E::Limit)? < meta.tail() + 1 - meta.head(),
        entries,
    })
}

fn owner_exists(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    name: &[u8],
) -> Result<bool, E> {
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            name,
        ],
        0,
        |cap| key::encode_credit_owner_key(scope, name, cap),
    )?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    Ok(read(tx, &table, &key, |_| Ok(()))?.is_some())
}

pub(super) fn catalog_key(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    sequence: Option<u64>,
) -> Result<Vec<u8>, E> {
    tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
        if sequence.is_some() { 8 } else { 0 },
        |cap| {
            sequence.map_or_else(
                || key::catalog::meta(scope, cap),
                |sequence| key::catalog::intent(scope, sequence, cap),
            )
        },
    )
}

fn read_meta(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
) -> Result<Option<CreditCatalogMeta>, E> {
    read(tx, table, &catalog_key(tx, scope, None)?, |bytes| {
        let meta = value::catalog::meta(bytes).map_err(value_error)?;
        if meta.tail() + 1 - meta.head() > 65_536 {
            return Err(E::Corrupt);
        }
        Ok(meta)
    })
}

fn read<T>(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
    decode: impl FnOnce(&[u8]) -> Result<T, E>,
) -> Result<Option<T>, E> {
    tx.step()?;
    let Some(bytes) = table.get(key).map_err(|e| storage_error(&e))? else {
        return Ok(None);
    };
    tx.recovery_row(key, bytes.value())?;
    tx.admit_read(total(64, [bytes.value().len()])?)?;
    decode(bytes.value()).map(Some)
}
