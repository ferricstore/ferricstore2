//! Closed detached reservation, page and cleanup values, with exact identities.

use super::{
    StateValueError as E, add, allocate, copy, finish, put_raw, raw, raw_size, read_u8, read_u16,
    read_u32, read_u64, start,
};
use wf_domain::governance::identified::{
    CreditCleanupTask, CreditPageData, CreditReservationStatus as S,
};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;

pub(crate) struct CreditReservationRow<'a> {
    pub(crate) id: &'a [u8],
    pub(crate) page: u16,
    pub(crate) status: S,
}

pub(crate) fn reservation(bytes: &[u8]) -> Result<CreditReservationRow<'_>, E> {
    let mut at = start(bytes, *b"FWGI", 269)?;
    let id = raw(bytes, &mut at)?;
    id_valid(id)?;
    let page = read_u16(bytes, &mut at)?;
    page_valid(page)?;
    let status = match read_u8(bytes, &mut at)? {
        0 => S::Active,
        1 => S::Released,
        _ => return Err(E::UnsupportedTag),
    };
    finish(bytes, at)?;
    Ok(CreditReservationRow { id, page, status })
}

pub(crate) fn encode_reservation(id: &[u8], page: u16, status: S) -> Result<Vec<u8>, E> {
    id_valid(id)?;
    page_valid(page)?;
    let mut out = allocate(
        raw_size(id)?.checked_add(9).ok_or(E::LengthOverflow)?,
        269,
        *b"FWGI",
    )?;
    put_raw(&mut out, id)?;
    out.extend_from_slice(&page.to_be_bytes());
    out.push(match status {
        S::Active => 0,
        S::Released => 1,
    });
    Ok(out)
}

pub(crate) fn page(bytes: &[u8], expected: (u64, u64, u16)) -> Result<CreditPageData, E> {
    let (at, count) = inspect_page(bytes, expected)?;
    let mut at = at;
    let mut ids = Vec::new();
    ids.try_reserve_exact(count).map_err(|_| E::Allocation)?;
    for _ in 0..count {
        ids.push(copy(raw(bytes, &mut at)?)?);
    }
    Ok(CreditPageData {
        shard_id: expected.0,
        epoch: expected.1,
        page: expected.2,
        reservation_ids: ids,
    })
}

pub(crate) fn inspect_page(bytes: &[u8], expected: (u64, u64, u16)) -> Result<(usize, usize), E> {
    identity(expected.0, expected.1)?;
    page_valid(expected.2)?;
    let mut at = start(bytes, *b"FWGP", 66_588)?;
    if (
        read_u64(bytes, &mut at)?,
        read_u64(bytes, &mut at)?,
        read_u16(bytes, &mut at)?,
    ) != expected
    {
        return Err(E::InvalidValue);
    }
    let count = usize::try_from(read_u32(bytes, &mut at)?).map_err(|_| E::LengthOverflow)?;
    if !(1..=256).contains(&count) {
        return Err(E::InvalidValue);
    }
    let ids_at = at;
    let mut sorted = [&[][..]; 256];
    for slot in &mut sorted[..count] {
        *slot = raw(bytes, &mut at)?;
        id_valid(slot)?;
    }
    finish(bytes, at)?;
    sorted[..count].sort_unstable();
    if sorted[..count].windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::InvalidValue);
    }
    Ok((ids_at, count))
}

pub(crate) fn encode_page(page: &CreditPageData) -> Result<Vec<u8>, E> {
    identity(page.shard_id, page.epoch)?;
    page_valid(page.page)?;
    if !(1..=256).contains(&page.reservation_ids.len()) {
        return Err(E::InvalidValue);
    }
    let mut size = 28;
    for id in &page.reservation_ids {
        id_valid(id)?;
        add(&mut size, raw_size(id)?)?;
    }
    let mut out = allocate(size, 66_588, *b"FWGP")?;
    for value in [page.shard_id, page.epoch] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out.extend_from_slice(&page.page.to_be_bytes());
    out.extend_from_slice(
        &u32::try_from(page.reservation_ids.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    for id in &page.reservation_ids {
        put_raw(&mut out, id)?;
    }
    inspect_page(&out, (page.shard_id, page.epoch, page.page))?;
    Ok(out)
}

pub(crate) fn cleanup(bytes: &[u8], sequence: u64) -> Result<CreditCleanupTask, E> {
    let mut at = start(bytes, *b"FWGQ", 34)?;
    if read_u64(bytes, &mut at)? != sequence || !(1..=MAX).contains(&sequence) {
        return Err(E::InvalidValue);
    }
    let task = CreditCleanupTask {
        shard_id: read_u64(bytes, &mut at)?,
        epoch: read_u64(bytes, &mut at)?,
        next_page: read_u16(bytes, &mut at)?,
        last_page: read_u16(bytes, &mut at)?,
    };
    finish(bytes, at)?;
    task_valid(task)?;
    Ok(task)
}

pub(crate) fn encode_cleanup(sequence: u64, task: CreditCleanupTask) -> Result<Vec<u8>, E> {
    if !(1..=MAX).contains(&sequence) {
        return Err(E::InvalidValue);
    }
    task_valid(task)?;
    let mut out = allocate(34, 34, *b"FWGQ")?;
    for value in [sequence, task.shard_id, task.epoch] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out.extend_from_slice(&task.next_page.to_be_bytes());
    out.extend_from_slice(&task.last_page.to_be_bytes());
    Ok(out)
}

fn task_valid(task: CreditCleanupTask) -> Result<(), E> {
    identity(task.shard_id, task.epoch)?;
    page_valid(task.next_page)?;
    page_valid(task.last_page)?;
    if task.next_page > task.last_page {
        return Err(E::InvalidValue);
    }
    Ok(())
}

fn identity(shard: u64, epoch: u64) -> Result<(), E> {
    if shard > MAX || !(1..=MAX).contains(&epoch) {
        return Err(E::InvalidValue);
    }
    Ok(())
}

fn page_valid(page: u16) -> Result<(), E> {
    if !(1..=256).contains(&page) {
        return Err(E::InvalidValue);
    }
    Ok(())
}

fn id_valid(id: &[u8]) -> Result<(), E> {
    if id.is_empty() || id.len() > 256 {
        return Err(E::InvalidValue);
    }
    Ok(())
}
