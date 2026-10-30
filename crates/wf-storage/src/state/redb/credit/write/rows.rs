//! Sparse writes preserve original page ownership and released tombstones.

use super::super::super::{
    admission::{storage_error, table_error, total, value_error},
    publication,
};
use super::super::CreditReadContext;
use crate::state::value;
use wf_domain::governance::identified::{
    CreditPageChange as P, CreditReservationStatus as S, IdentifiedCreditPlan,
};
use wf_ports::log::encode_credit_owner;
use wf_ports::state::StateTransactionError as E;

struct CreditPageBinding<'a> {
    shard: u64,
    epoch: u64,
    id: &'a [u8],
    page: u16,
}

pub(super) fn publish(
    context: &CreditReadContext<'_, '_>,
    plan: &IdentifiedCreditPlan<'_>,
) -> Result<(), E> {
    let bindings = bindings(context, plan)?;
    for change in plan.reservations() {
        let key = context.reservation_key(change.shard_id, change.epoch, &change.id)?;
        let old = context.read(&key, |bytes| {
            let row = value::credit::reservation(bytes).map_err(value_error)?;
            if row.id != change.id.as_ref() {
                return Err(E::Corrupt);
            }
            Ok((row.page, row.status))
        })?;
        let page = match change.status {
            Some(S::Active) => {
                if old.is_some() {
                    return Err(E::InvalidChanges);
                }
                let at = bindings
                    .binary_search_by(|row| {
                        (row.shard, row.epoch, row.id).cmp(&(
                            change.shard_id,
                            change.epoch,
                            change.id.as_ref(),
                        ))
                    })
                    .map_err(|_| E::InvalidChanges)?;
                bindings[at].page
            }
            Some(S::Released) => {
                let (page, S::Active) = old.ok_or(E::Corrupt)? else {
                    return Err(E::Corrupt);
                };
                page
            }
            None => {
                let (page, _) = old.ok_or(E::Corrupt)?;
                if !plan.pages().iter().any(|p| {
                    matches!(p, P::Delete { shard_id, epoch, page: target }
                    if (*shard_id, *epoch, *target) == (change.shard_id, change.epoch, page))
                }) {
                    return Err(E::InvalidChanges);
                }
                remove(context, &key)?;
                continue;
            }
        };
        context.tx.temporary(269)?;
        let bytes = value::credit::encode_reservation(
            &change.id,
            page,
            change.status.ok_or(E::InvalidChanges)?,
        )
        .map_err(value_error)?;
        publication::put(context.tx, &key, &bytes)?;
    }
    for change in plan.pages() {
        match change {
            P::Put(page) => {
                let key = context.page_key(page.shard_id, page.epoch, page.page)?;
                let size = page
                    .reservation_ids
                    .iter()
                    .try_fold(28_usize, |size, id| total(size, [4, id.len()]))?;
                context.tx.temporary(total(size, [8192])?)?;
                let bytes = value::credit::encode_page(page).map_err(value_error)?;
                publication::put(context.tx, &key, &bytes)?;
            }
            P::Delete {
                shard_id,
                epoch,
                page,
            } => remove(context, &context.page_key(*shard_id, *epoch, *page)?)?,
        }
    }
    for change in plan.cleanup() {
        let key = context.cleanup_key(change.sequence)?;
        if let Some(task) = change.task {
            context.tx.temporary(34)?;
            let bytes =
                value::credit::encode_cleanup(change.sequence, task).map_err(value_error)?;
            publication::put(context.tx, &key, &bytes)?;
        } else {
            remove(context, &key)?;
        }
    }
    if let Some(owner) = plan.changed_owner() {
        let bound = owner
            .view()
            .leases
            .len()
            .checked_mul(69)
            .and_then(|n| n.checked_add(owner.view().scope.len()))
            .and_then(|n| n.checked_add(100))
            .ok_or(E::Limit)?;
        let maximum_owned = total(bound, [2048])?;
        context.tx.temporary(maximum_owned)?;
        let bytes = encode_credit_owner(owner, bound, maximum_owned)
            .map_err(super::super::read::codec_error)?;
        publication::put(context.tx, &context.owner_key()?, &bytes)?;
    }
    Ok(())
}

fn bindings<'a>(
    context: &CreditReadContext<'_, '_>,
    plan: &'a IdentifiedCreditPlan<'_>,
) -> Result<Vec<CreditPageBinding<'a>>, E> {
    let count = plan.pages().iter().try_fold(0_usize, |count, page| {
        total(
            count,
            [match page {
                P::Put(page) => page.reservation_ids.len(),
                P::Delete { .. } => 0,
            }],
        )
    })?;
    if count > 1256 {
        return Err(E::InvalidChanges);
    }
    context.tx.temporary(
        count
            .checked_mul(std::mem::size_of::<CreditPageBinding<'_>>())
            .ok_or(E::Limit)?,
    )?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| E::Allocation)?;
    for change in plan.pages() {
        if let P::Put(page) = change {
            for id in &page.reservation_ids {
                rows.push(CreditPageBinding {
                    shard: page.shard_id,
                    epoch: page.epoch,
                    id,
                    page: page.page,
                });
            }
        }
    }
    rows.sort_unstable_by(|a, b| (a.shard, a.epoch, a.id).cmp(&(b.shard, b.epoch, b.id)));
    if rows
        .windows(2)
        .any(|p| (p[0].shard, p[0].epoch, p[0].id) == (p[1].shard, p[1].epoch, p[1].id))
    {
        return Err(E::InvalidChanges);
    }
    Ok(rows)
}

fn remove(context: &CreditReadContext<'_, '_>, key: &[u8]) -> Result<(), E> {
    context.tx.step()?;
    let mut table = context
        .tx
        .write
        .open_table(context.tx.rows_table)
        .map_err(table_error)?;
    if table.remove(key).map_err(|e| storage_error(&e))?.is_none() {
        return Err(E::Corrupt);
    }
    Ok(())
}
