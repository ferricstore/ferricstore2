//! Sparse membership classification before capacity and page effects.

use super::{
    CreditProtocolObservation as Observation, CreditReservationChange,
    CreditReservationObservation, CreditReservationStatus as Status, IdentifiedCreditDecision as D,
    IdentifiedCreditMutation as Op, IdentifiedCreditRejection as R, cleanup, pages, prepare,
    protocol::{CreditPhase, PreparedIdentifiedCredit},
};
use crate::governance::credit::{
    CreditDecision, CreditError as E, CreditOperation, CreditRejection,
    admission::{product, reserve},
    decision,
};
use std::borrow::Cow;

pub(super) fn resume<'a>(
    mut prepared: PreparedIdentifiedCredit<'a>,
    observation: Observation<'a>,
) -> Result<PreparedIdentifiedCredit<'a>, E> {
    prepared.charge.work(256)?;
    match (prepared.phase, observation) {
        (CreditPhase::Reservations { shard_id, epoch }, Observation::Reservations(values)) => {
            prepared.charge.own(product(
                values.capacity(),
                std::mem::size_of::<CreditReservationObservation<'_>>(),
            )?)?;
            membership(&mut prepared, shard_id, epoch, &values)?;
        }
        (CreditPhase::SpendPage { .. }, Observation::Page(page)) => {
            pages::append(&mut prepared, page)?;
        }
        (CreditPhase::CleanupTask { .. }, Observation::CleanupTask(task)) => {
            cleanup::observed_task(&mut prepared, task)?;
        }
        (CreditPhase::CleanupPage { task }, Observation::Page(page)) => {
            cleanup::observed_page(&mut prepared, task, page)?;
        }
        _ => return Err(E::InvalidState),
    }
    Ok(prepared)
}

fn membership<'a>(
    prepared: &mut PreparedIdentifiedCredit<'a>,
    shard: u64,
    epoch: u64,
    values: &[CreditReservationObservation<'a>],
) -> Result<(), E> {
    let ids = prepared.original_ids();
    if values.len() != ids.len() {
        return Err(E::InvalidState);
    }
    let mut known = 0;
    for (value, id) in values.iter().zip(ids) {
        prepared.charge.work(
            128_usize
                .checked_add(id.len())
                .and_then(|n| n.checked_add(value.id.len()))
                .ok_or(E::Limit)?,
        )?;
        if value.id != id || value.shard_id != shard || value.epoch != epoch {
            return Err(E::InvalidState);
        }
        known += usize::from(value.status.is_some());
    }
    match prepared.request.mutation {
        Op::Spend { .. } => spend(prepared, shard, known),
        Op::Release { .. } => release(prepared, shard, epoch, values),
        _ => Err(E::InvalidState),
    }
}

fn spend(prepared: &mut PreparedIdentifiedCredit<'_>, shard: u64, known: usize) -> Result<(), E> {
    prepared.phase = CreditPhase::Complete;
    if known == prepared.original_ids().len() {
        prepared.decision = D::Spent { shard_id: shard };
        return Ok(());
    }
    if known != 0 {
        prepared.decision = D::Rejected(R::Credit(CreditRejection::ReservationConflict));
        return Ok(());
    }
    let Op::Spend { amount, ttl_ms, .. } = prepared.request.mutation else {
        return Err(E::InvalidState);
    };
    let result = decision::apply(
        prepared.data.as_mut().ok_or(E::InvalidState)?,
        CreditOperation::Spend {
            shard_id: shard,
            amount,
            now: prepared.now,
            ttl_ms,
            reservation_ids: &[],
        },
        &mut prepared.charge,
    )?;
    prepare::classify(prepared, result, D::Spent { shard_id: shard });
    if !matches!(result, CreditDecision::Spent { .. }) {
        return Ok(());
    }
    let index = prepare::locate(prepared, shard)?.ok_or(E::InvalidState)?;
    let lease = &prepared.data.as_ref().ok_or(E::InvalidState)?.leases[index];
    let entries = if lease.reservation_page == 0 {
        0
    } else {
        (usize::from(lease.reservation_page) - 1) * 256 + usize::from(lease.reservation_page_fill)
    };
    if entries
        .checked_add(prepared.original_ids().len())
        .ok_or(E::Limit)?
        > 65_536
    {
        prepare::abort(prepared, R::ReservationHistoryFull);
        return Ok(());
    }
    if lease.reservation_page != 0 && lease.reservation_page_fill < 256 {
        prepared.phase = CreditPhase::SpendPage {
            shard_id: shard,
            epoch: lease.epoch,
            page: lease.reservation_page,
        };
        Ok(())
    } else {
        pages::append(prepared, None)
    }
}

fn release<'a>(
    prepared: &mut PreparedIdentifiedCredit<'a>,
    shard: u64,
    epoch: u64,
    values: &[CreditReservationObservation<'a>],
) -> Result<(), E> {
    let active = values
        .iter()
        .filter(|value| value.status == Some(Status::Active))
        .count();
    prepared.charge.work(product(values.len(), 128)?)?;
    let index = prepare::locate(prepared, shard)?.ok_or(E::InvalidState)?;
    if u64::try_from(active).map_err(|_| E::Limit)?
        > prepared.data.as_ref().ok_or(E::InvalidState)?.leases[index].in_use
    {
        return Err(E::InvalidState);
    }
    reserve(&mut prepared.reservations, active, &mut prepared.charge)?;
    decision::apply(
        prepared.data.as_mut().ok_or(E::InvalidState)?,
        CreditOperation::ReleaseIdentifiedAmount {
            shard_id: shard,
            amount: u64::try_from(active).map_err(|_| E::Limit)?,
        },
        &mut prepared.charge,
    )?;
    for value in values
        .iter()
        .filter(|value| value.status == Some(Status::Active))
    {
        prepared.reservations.push(CreditReservationChange {
            id: Cow::Borrowed(value.id),
            shard_id: shard,
            epoch,
            status: Some(Status::Released),
        });
    }
    prepared.phase = CreditPhase::Complete;
    prepared.decision = D::Released;
    cleanup::retire(prepared, shard)?;
    Ok(())
}
