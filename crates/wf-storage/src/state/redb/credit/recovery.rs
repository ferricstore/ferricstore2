//! Forward closure plus an injective page/ID census proves the complete cohort.

use super::super::{
    admission::{storage_error, total, value_error},
    store::RedbStateTransaction,
};
use super::CreditReadContext;
use crate::state::{key::DecodedStateKey, value};
use redb::ReadableTable;
use wf_domain::governance::credit::CreditOwner;
use wf_domain::governance::identified::{CreditCleanupTask, CreditReservationStatus as S};
use wf_ports::state::StateTransactionError as E;

pub(in super::super) fn row(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
    shards: u64,
) -> Result<(u64, u64), E> {
    let scope = tx.scope_bytes(key.part(2))?;
    let context = CreditReadContext::recovered(tx, &scope, key.part(3), shards);
    match key.family() {
        12 => {
            let owner = context.decode_owner(bytes)?;
            forward(&context, table, &owner)?;
        }
        13 => {
            let row = value::credit::reservation(bytes).map_err(value_error)?;
            if row.id != key.part(6) || scalar(key.part(4))? >= shards {
                return Err(E::Corrupt);
            }
            return Ok((1, 0));
        }
        14 => {
            tx.temporary(8192)?;
            let shard = scalar(key.part(4))?;
            let epoch = scalar(key.part(5))?;
            let page = u16::try_from(key.number()).map_err(|_| E::Corrupt)?;
            let (_, count) =
                value::credit::inspect_page(bytes, (shard, epoch, page)).map_err(value_error)?;
            let owner = owner(&context, table)?;
            if shard >= shards || !covered(&context, table, &owner, shard, epoch, page)? {
                return Err(E::Corrupt);
            }
            return Ok((0, u64::try_from(count).map_err(|_| E::Limit)?));
        }
        15 => {
            let sequence = u64::try_from(key.number()).map_err(|_| E::Corrupt)?;
            let task = value::credit::cleanup(bytes, sequence).map_err(value_error)?;
            let owner = owner(&context, table)?;
            if sequence < owner.view().cleanup_head || sequence > owner.view().cleanup_tail {
                return Err(E::Corrupt);
            }
            task_valid(&owner, task, shards)?;
        }
        _ => return Err(E::Corrupt),
    }
    Ok((0, 0))
}

fn forward(
    context: &CreditReadContext<'_, '_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    owner: &CreditOwner,
) -> Result<(), E> {
    let data = owner.view();
    let pending = data.cleanup_tail + 1 - data.cleanup_head;
    if pending > 256 {
        return Err(E::Corrupt);
    }
    let count = total(
        data.leases.len(),
        [usize::try_from(pending).map_err(|_| E::Limit)?],
    )?;
    context.tx.temporary(
        count
            .checked_mul(std::mem::size_of::<u64>())
            .ok_or(E::Limit)?,
    )?;
    let mut generations = Vec::new();
    generations
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    for lease in &data.leases {
        generations.push(lease.epoch);
        let mut active = 0_u64;
        for page in 1..=lease.reservation_page {
            let expected = if page == lease.reservation_page {
                lease.reservation_page_fill
            } else {
                256
            };
            let (found, held) = members(context, table, lease.shard_id, lease.epoch, page)?;
            if found != usize::from(expected) {
                return Err(E::Corrupt);
            }
            active = active.checked_add(held).ok_or(E::Limit)?;
        }
        if active != lease.in_use {
            return Err(E::Corrupt);
        }
    }
    for sequence in data.cleanup_head..=data.cleanup_tail {
        let task = task(context, table, sequence)?;
        task_valid(owner, task, context.shards)?;
        generations.push(task.epoch);
        for page in task.next_page..=task.last_page {
            let (count, _) = members(context, table, task.shard_id, task.epoch, page)?;
            if page != task.last_page && count != 256 {
                return Err(E::Corrupt);
            }
        }
    }
    generations.sort_unstable();
    if generations.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::Corrupt);
    }
    Ok(())
}

fn members(
    context: &CreditReadContext<'_, '_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    shard: u64,
    epoch: u64,
    page: u16,
) -> Result<(usize, u64), E> {
    let key = context.page_key(shard, epoch, page)?;
    context.tx.temporary(8192)?;
    read(context, table, &key, |bytes| {
        let (mut at, count) =
            value::credit::inspect_page(bytes, (shard, epoch, page)).map_err(value_error)?;
        let mut active = 0_u64;
        for _ in 0..count {
            let id = value::raw(bytes, &mut at).map_err(value_error)?;
            let key = context.reservation_key(shard, epoch, id)?;
            let status = read(context, table, &key, |bytes| member_status(bytes, id, page))?;
            if status == S::Active {
                active = active.checked_add(1).ok_or(E::Limit)?;
            }
        }
        Ok((count, active))
    })
}

fn member_status(bytes: &[u8], id: &[u8], page: u16) -> Result<S, E> {
    let row = value::credit::reservation(bytes).map_err(value_error)?;
    if row.id != id || row.page != page {
        return Err(E::Corrupt);
    }
    Ok(row.status)
}

fn covered(
    context: &CreditReadContext<'_, '_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    owner: &CreditOwner,
    shard: u64,
    epoch: u64,
    page: u16,
) -> Result<bool, E> {
    if owner
        .lease(shard)
        .is_some_and(|lease| lease.epoch == epoch && page <= lease.reservation_page)
    {
        return Ok(true);
    }
    if owner.view().cleanup_tail + 1 - owner.view().cleanup_head > 256 {
        return Err(E::Corrupt);
    }
    for sequence in owner.view().cleanup_head..=owner.view().cleanup_tail {
        let task = task(context, table, sequence)?;
        if task.shard_id == shard
            && task.epoch == epoch
            && (task.next_page..=task.last_page).contains(&page)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn owner(
    context: &CreditReadContext<'_, '_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
) -> Result<CreditOwner, E> {
    read(context, table, &context.owner_key()?, |bytes| {
        context.decode_owner(bytes)
    })
}

fn task(
    context: &CreditReadContext<'_, '_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    sequence: u64,
) -> Result<CreditCleanupTask, E> {
    read(context, table, &context.cleanup_key(sequence)?, |bytes| {
        value::credit::cleanup(bytes, sequence).map_err(value_error)
    })
}

fn read<T>(
    context: &CreditReadContext<'_, '_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
    decode: impl FnOnce(&[u8]) -> Result<T, E>,
) -> Result<T, E> {
    context.tx.step()?;
    let value = table
        .get(key)
        .map_err(|e| storage_error(&e))?
        .ok_or(E::Corrupt)?;
    decode(value.value())
}

fn task_valid(owner: &CreditOwner, task: CreditCleanupTask, shards: u64) -> Result<(), E> {
    if task.shard_id >= shards
        || task.epoch > owner.view().epoch
        || owner
            .lease(task.shard_id)
            .is_some_and(|lease| lease.epoch == task.epoch)
    {
        return Err(E::Corrupt);
    }
    Ok(())
}

fn scalar(bytes: &[u8]) -> Result<u64, E> {
    Ok(u64::from_be_bytes(
        bytes.try_into().map_err(|_| E::Corrupt)?,
    ))
}
