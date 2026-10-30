//! Bounded ordered page mutation; old payload owners move into the final plan.

use super::{
    CreditPageChange, CreditPageData, CreditReservationChange, CreditReservationStatus,
    IdentifiedCreditDecision as D, IdentifiedCreditMutation as Op, prepare,
    protocol::{CreditPhase, PreparedIdentifiedCredit},
};
use crate::governance::credit::{
    CreditError as E,
    admission::{CreditCharge, product, reserve},
};
use std::{borrow::Cow, cmp::Ordering};

pub(super) fn validate<'a>(
    page: &'a CreditPageData,
    expected: (u64, u64, u16),
    charge: &mut CreditCharge,
) -> Result<Vec<&'a [u8]>, E> {
    if (page.shard_id, page.epoch, page.page) != expected
        || page.reservation_ids.is_empty()
        || page.reservation_ids.len() > 256
    {
        return Err(E::InvalidState);
    }
    charge.input_ids(page.reservation_ids.len())?;
    charge.own(
        std::mem::size_of::<CreditPageData>()
            .checked_add(product(
                page.reservation_ids.capacity(),
                std::mem::size_of::<Vec<u8>>(),
            )?)
            .ok_or(E::Limit)?,
    )?;
    charge.work(product(page.reservation_ids.len(), 16_384)?)?;
    let mut ordered = Vec::new();
    reserve(&mut ordered, page.reservation_ids.len(), charge)?;
    for id in &page.reservation_ids {
        charge.own(id.capacity())?;
        if id.is_empty() || id.len() > 256 {
            return Err(E::InvalidState);
        }
        ordered.push(id.as_slice());
    }
    ordered.sort_unstable();
    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::InvalidState);
    }
    Ok(ordered)
}

pub(super) fn append(
    prepared: &mut PreparedIdentifiedCredit<'_>,
    supplied: Option<CreditPageData>,
) -> Result<(), E> {
    let Op::Spend { shard_id, .. } = prepared.request.mutation else {
        return Err(E::InvalidState);
    };
    let ids = prepared.original_ids();
    let index = prepare::locate(prepared, shard_id)?.ok_or(E::InvalidState)?;
    let lease = &prepared.data.as_ref().ok_or(E::InvalidState)?.leases[index];
    let (epoch, old_page, old_fill) = (
        lease.epoch,
        lease.reservation_page,
        usize::from(lease.reservation_page_fill),
    );
    let mut page = if old_page != 0 && old_fill < 256 {
        let supplied = supplied.ok_or(E::InvalidState)?;
        let ordered = validate(&supplied, (shard_id, epoch, old_page), &mut prepared.charge)?;
        if supplied.reservation_ids.len() != old_fill {
            return Err(E::InvalidState);
        }
        for id in ids {
            absent(&ordered, id, &mut prepared.charge)?;
        }
        drop(ordered);
        supplied
    } else {
        if supplied.is_some() {
            return Err(E::InvalidState);
        }
        CreditPageData {
            shard_id,
            epoch,
            page: old_page
                .checked_add(1)
                .filter(|n| *n <= 256)
                .ok_or(E::InvalidState)?,
            reservation_ids: Vec::new(),
        }
    };
    let space = 256 - page.reservation_ids.len();
    let effects = 1_usize
        .checked_add(ids.len().saturating_sub(space).div_ceil(256))
        .ok_or(E::Limit)?;
    reserve(&mut prepared.pages, effects, &mut prepared.charge)?;
    reserve(&mut prepared.reservations, ids.len(), &mut prepared.charge)?;
    reserve(
        &mut page.reservation_ids,
        ids.len().min(space),
        &mut prepared.charge,
    )?;
    for (ordinal, id) in ids.iter().enumerate() {
        if page.reservation_ids.len() == 256 {
            let next = page
                .page
                .checked_add(1)
                .filter(|n| *n <= 256)
                .ok_or(E::InvalidState)?;
            prepared.pages.push(CreditPageChange::Put(page));
            page = CreditPageData {
                shard_id,
                epoch,
                page: next,
                reservation_ids: Vec::new(),
            };
            reserve(
                &mut page.reservation_ids,
                (ids.len() - ordinal).min(256),
                &mut prepared.charge,
            )?;
        }
        prepared
            .charge
            .work(id.len().checked_add(128).ok_or(E::Limit)?)?;
        prepared.charge.own(id.len())?;
        let mut copied = Vec::new();
        copied
            .try_reserve_exact(id.len())
            .map_err(|_| E::Allocation)?;
        copied.extend_from_slice(id);
        page.reservation_ids.push(copied);
        prepared.reservations.push(CreditReservationChange {
            id: Cow::Borrowed(id),
            shard_id,
            epoch,
            status: Some(CreditReservationStatus::Active),
        });
    }
    let lease = &mut prepared.data.as_mut().ok_or(E::InvalidState)?.leases[index];
    lease.reservation_page = page.page;
    lease.reservation_page_fill =
        u16::try_from(page.reservation_ids.len()).map_err(|_| E::Limit)?;
    prepared.pages.push(CreditPageChange::Put(page));
    prepared.phase = CreditPhase::Complete;
    prepared.decision = D::Spent { shard_id };
    Ok(())
}

fn absent(ordered: &[&[u8]], id: &[u8], charge: &mut CreditCharge) -> Result<(), E> {
    let (mut low, mut high) = (0, ordered.len());
    while low < high {
        let mid = low + (high - low) / 2;
        charge.work(
            ordered[mid]
                .len()
                .checked_add(id.len())
                .and_then(|n| n.checked_add(16))
                .ok_or(E::Limit)?,
        )?;
        match ordered[mid].cmp(id) {
            Ordering::Equal => return Err(E::InvalidState),
            Ordering::Less => low = mid + 1,
            Ordering::Greater => high = mid,
        }
    }
    Ok(())
}
