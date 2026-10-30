//! Ordered retired-generation tasks and one-page cleanup planning.

use super::{
    CreditCleanupChange, CreditCleanupTask, CreditPageChange, CreditPageData,
    CreditReservationChange, IdentifiedCreditDecision as D, IdentifiedCreditRejection as R, pages,
    prepare,
    protocol::{CreditPhase, PreparedIdentifiedCredit},
};
use crate::governance::credit::{
    CreditError as E, CreditRejection,
    admission::{product, reserve},
    capacity,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;
use std::borrow::Cow;

pub(super) fn expire(prepared: &mut PreparedIdentifiedCredit<'_>) -> Result<bool, E> {
    let data = prepared.data.as_ref().ok_or(E::InvalidState)?;
    prepared.charge.work(product(data.leases.len(), 256)?)?;
    let count = data
        .leases
        .iter()
        .filter(|lease| lease.expires_at_ms <= prepared.now.get() && lease.reservation_page != 0)
        .count();
    let queued = pending(data.cleanup_head, data.cleanup_tail)?;
    if queued.checked_add(count).ok_or(E::Limit)? > 256 {
        if queued != 0
            && matches!(
                prepared.request.mutation,
                super::IdentifiedCreditMutation::Cleanup
            )
        {
            // Drain existing work before admitting another generation; its lease
            // stays intact until the detached reference can be queued safely.
            return Ok(true);
        }
        prepare::abort(prepared, R::CleanupBacklogFull);
        return Ok(false);
    }
    reserve(&mut prepared.cleanup, count, &mut prepared.charge)?;
    let length = data.leases.len();
    for index in 0..length {
        let lease = &prepared.data.as_ref().ok_or(E::InvalidState)?.leases[index];
        if lease.expires_at_ms <= prepared.now.get() && lease.reservation_page != 0 {
            let task = CreditCleanupTask {
                shard_id: lease.shard_id,
                epoch: lease.epoch,
                next_page: 1,
                last_page: lease.reservation_page,
            };
            enqueue(prepared, task)?;
        }
    }
    capacity::expire(
        prepared.data.as_mut().ok_or(E::InvalidState)?,
        prepared.now.get(),
        &mut prepared.charge,
    )?;
    Ok(true)
}

pub(super) fn retire(prepared: &mut PreparedIdentifiedCredit<'_>, shard: u64) -> Result<bool, E> {
    let Some(index) = prepare::locate(prepared, shard)? else {
        return Ok(true);
    };
    let data = prepared.data.as_ref().ok_or(E::InvalidState)?;
    let lease = &data.leases[index];
    if lease.in_use != 0
        || lease.reservation_page == 0
        || pending(data.cleanup_head, data.cleanup_tail)? >= 256
    {
        return Ok(true);
    }
    if data.epoch == MAX {
        prepare::abort(prepared, R::Credit(CreditRejection::GenerationExhausted));
        return Ok(false);
    }
    let task = CreditCleanupTask {
        shard_id: shard,
        epoch: lease.epoch,
        next_page: 1,
        last_page: lease.reservation_page,
    };
    enqueue(prepared, task)?;
    let data = prepared.data.as_mut().ok_or(E::InvalidState)?;
    data.epoch += 1;
    data.leases[index].epoch = data.epoch;
    data.leases[index].reservation_page = 0;
    data.leases[index].reservation_page_fill = 0;
    Ok(true)
}

fn enqueue(prepared: &mut PreparedIdentifiedCredit<'_>, task: CreditCleanupTask) -> Result<(), E> {
    let data = prepared.data.as_mut().ok_or(E::InvalidState)?;
    let sequence = data
        .cleanup_tail
        .checked_add(1)
        .filter(|n| *n <= MAX)
        .ok_or(E::Limit)?;
    reserve(&mut prepared.cleanup, 1, &mut prepared.charge)?;
    prepared.cleanup.push(CreditCleanupChange {
        sequence,
        task: Some(task),
    });
    data.cleanup_tail = sequence;
    Ok(())
}

pub(super) fn start(prepared: &mut PreparedIdentifiedCredit<'_>) -> Result<(), E> {
    let data = prepared.data.as_ref().ok_or(E::InvalidState)?;
    if data.cleanup_head > data.cleanup_tail {
        prepared.phase = CreditPhase::Complete;
        prepared.decision = D::Cleaned {
            deleted: 0,
            pending: false,
        };
        return Ok(());
    }
    prepared
        .charge
        .work(product(prepared.cleanup.len(), 128)?)?;
    let head = data.cleanup_head;
    if let Some(task) = prepared
        .cleanup
        .iter()
        .find(|change| change.sequence == head)
        .and_then(|change| change.task)
    {
        prepared.phase = CreditPhase::CleanupPage { task };
    } else {
        prepared.phase = CreditPhase::CleanupTask { sequence: head };
    }
    Ok(())
}

pub(super) fn observed_task(
    prepared: &mut PreparedIdentifiedCredit<'_>,
    task: Option<CreditCleanupTask>,
) -> Result<(), E> {
    let task = task.ok_or(E::InvalidState)?;
    let data = prepared.data.as_ref().ok_or(E::InvalidState)?;
    prepared.charge.work(product(data.leases.len(), 128)?)?;
    if task.shard_id >= prepared.request.shard_count
        || task.epoch == 0
        || task.epoch > data.epoch
        || task.next_page == 0
        || task.next_page > task.last_page
        || task.last_page > 256
        || data
            .leases
            .iter()
            .any(|lease| lease.shard_id == task.shard_id && lease.epoch == task.epoch)
    {
        return Err(E::InvalidState);
    }
    prepared.phase = CreditPhase::CleanupPage { task };
    Ok(())
}

pub(super) fn observed_page(
    prepared: &mut PreparedIdentifiedCredit<'_>,
    task: CreditCleanupTask,
    page: Option<CreditPageData>,
) -> Result<(), E> {
    let page = page.ok_or(E::InvalidState)?;
    drop(pages::validate(
        &page,
        (task.shard_id, task.epoch, task.next_page),
        &mut prepared.charge,
    )?);
    let deleted = u16::try_from(page.reservation_ids.len()).map_err(|_| E::Limit)?;
    reserve(
        &mut prepared.reservations,
        page.reservation_ids.len(),
        &mut prepared.charge,
    )?;
    reserve(&mut prepared.pages, 1, &mut prepared.charge)?;
    reserve(&mut prepared.cleanup, 1, &mut prepared.charge)?;
    let data = prepared.data.as_mut().ok_or(E::InvalidState)?;
    let sequence = data.cleanup_head;
    let next_task = if task.next_page == task.last_page {
        data.cleanup_head = data
            .cleanup_head
            .checked_add(1)
            .filter(|n| *n <= MAX)
            .ok_or(E::Limit)?;
        None
    } else {
        Some(CreditCleanupTask {
            next_page: task.next_page + 1,
            ..task
        })
    };
    for id in page.reservation_ids {
        prepared.reservations.push(CreditReservationChange {
            id: Cow::Owned(id),
            shard_id: task.shard_id,
            epoch: task.epoch,
            status: None,
        });
    }
    prepared.pages.push(CreditPageChange::Delete {
        shard_id: task.shard_id,
        epoch: task.epoch,
        page: task.next_page,
    });
    prepared.cleanup.push(CreditCleanupChange {
        sequence,
        task: next_task,
    });
    prepared.decision = D::Cleaned {
        deleted,
        pending: data.cleanup_head <= data.cleanup_tail,
    };
    prepared.phase = CreditPhase::Complete;
    Ok(())
}

fn pending(head: u64, tail: u64) -> Result<usize, E> {
    usize::try_from(
        tail.checked_add(1)
            .and_then(|n| n.checked_sub(head))
            .ok_or(E::InvalidState)?,
    )
    .map_err(|_| E::Limit)
}
