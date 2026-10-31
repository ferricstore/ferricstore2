//! Complete borrowed response inspection followed by bounded owned restoration.

use super::{
    admission,
    owner::{self, ScannedCreditOwner},
    status,
};
use crate::log::{
    CodecError as E, CodecLimits,
    reader::{Reader, copy},
    writer::Writer,
};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::credit::CreditOwner;
use wf_domain::governance::identified::{
    IdentifiedCreditDecision as D, IdentifiedCreditRejection as R, IdentifiedCreditReply,
};

pub(in crate::log) struct ScannedCreditReply<'a> {
    encoded: &'a [u8],
    decision: D,
    owner: Option<ScannedCreditOwner<'a>>,
    ids_at: usize,
    count: usize,
    owned: usize,
}

impl ScannedCreditReply<'_> {
    pub(in crate::log) fn owner_scope(&self) -> Option<&[u8]> {
        self.owner.as_ref().map(ScannedCreditOwner::scope)
    }
    pub(in crate::log) const fn summary(&self) -> (D, usize) {
        (self.decision, self.count)
    }

    pub(in crate::log) fn matches_ids(
        &self,
        expected: wf_domain::governance::caller::CreditIdList<'_>,
    ) -> Result<bool, E> {
        if expected.len() != self.count {
            return Ok(false);
        }
        let mut reader = Reader::new(&self.encoded[self.ids_at..], CodecLimits::default());
        for id in expected.iter() {
            if reader.nonempty(256)? != id {
                return Ok(false);
            }
        }
        reader.finish()?;
        Ok(true)
    }

    pub(super) fn into_owned(self, maximum_owned: usize) -> Result<IdentifiedCreditReply, E> {
        if self.owned > maximum_owned {
            return Err(E::OwnedLimit);
        }
        let mut ids = Vec::new();
        ids.try_reserve_exact(self.count)
            .map_err(|_| E::Allocation)?;
        let mut reader = Reader::new(&self.encoded[self.ids_at..], CodecLimits::default());
        for _ in 0..self.count {
            ids.push(copy(reader.nonempty(256)?)?);
        }
        reader.finish()?;
        let owner = self
            .owner
            .map(|owner| owner.into_owned(maximum_owned))
            .transpose()?;
        Ok(IdentifiedCreditReply {
            decision: self.decision,
            owner,
            reservation_ids: ids,
        })
    }
}

pub(in crate::log) fn inspect(
    encoded: &[u8],
    credit: CreditLimits,
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<ScannedCreditReply<'_>, E> {
    if encoded.len() > maximum_wire {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, CodecLimits::default());
    if reader.take(4)? != b"FWGR" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    let decision = status::read(&mut reader)?;
    let owner = reader.optional(|reader| {
        owner::inspect(
            reader.nonempty(900_000)?,
            credit,
            maximum_wire,
            maximum_owned,
        )
    })?;
    let count = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
    if count > 1000 || count > reader.remaining() / 5 {
        return Err(E::Malformed);
    }
    if count > credit.max_reservations() {
        return Err(E::ItemLimit);
    }
    let ids_at = encoded.len() - reader.remaining();
    let mut payload = 0_usize;
    for _ in 0..count {
        payload = payload
            .checked_add(reader.nonempty(256)?.len())
            .ok_or(E::LengthOverflow)?;
    }
    reader.finish()?;
    let required_bytes = count
        .checked_mul(std::mem::size_of::<Vec<u8>>())
        .and_then(|n| n.checked_add(payload))
        .and_then(|n| n.checked_add(owner.as_ref().map_or(0, ScannedCreditOwner::owned_bytes)))
        .and_then(|n| n.checked_add(2048))
        .ok_or(E::LengthOverflow)?;
    if required_bytes > maximum_owned.min(credit.max_owned_bytes()) {
        return Err(E::OwnedLimit);
    }
    let work = encoded
        .len()
        .checked_mul(8)
        .and_then(|n| count.checked_mul(16_384).and_then(|ids| n.checked_add(ids)))
        .and_then(|n| n.checked_add(4096))
        .ok_or(E::LengthOverflow)?;
    if u64::try_from(work).map_err(|_| E::LengthOverflow)? > credit.max_work() {
        return Err(E::WorkLimit);
    }
    validate_scanned(decision, owner.as_ref(), count)?;
    unique(encoded, ids_at, count)?;
    Ok(ScannedCreditReply {
        encoded,
        decision,
        owner,
        ids_at,
        count,
        owned: required_bytes,
    })
}

fn unique(encoded: &[u8], at: usize, count: usize) -> Result<(), E> {
    let mut reader = Reader::new(&encoded[at..], CodecLimits::default());
    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    for _ in 0..count {
        ordered.push(reader.nonempty(256)?);
    }
    ordered.sort_unstable();
    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::Malformed);
    }
    Ok(())
}

fn validate_scanned(
    decision: D,
    owner: Option<&ScannedCreditOwner<'_>>,
    count: usize,
) -> Result<(), E> {
    let spent = matches!(decision, D::Spent { .. });
    if spent != (count != 0) {
        return Err(E::Malformed);
    }
    match decision {
        D::Granted { shard_id } | D::Spent { shard_id } | D::Renewed { shard_id } => {
            if !owner.ok_or(E::Malformed)?.has_lease(shard_id)? {
                return Err(E::Malformed);
            }
        }
        D::Released | D::Owner => {
            owner.ok_or(E::Malformed)?;
        }
        D::Cleaned { pending, .. } => {
            if owner.ok_or(E::Malformed)?.cleanup_pending() != pending {
                return Err(E::Malformed);
            }
        }
        D::Rejected(R::NotFound) if owner.is_some() => return Err(E::Malformed),
        D::Rejected(_) => {}
    }
    Ok(())
}

pub(super) fn encode(
    reply: &IdentifiedCreditReply,
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<Vec<u8>, E> {
    encode_parts(
        reply.decision,
        reply.owner.as_ref(),
        &reply.reservation_ids,
        maximum_wire,
        maximum_owned,
    )
}

pub(super) fn encode_parts(
    decision: D,
    owner: Option<&CreditOwner>,
    ids: &[Vec<u8>],
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<Vec<u8>, E> {
    validate_owned(decision, owner, ids, maximum_owned)?;
    let mut measure = Writer::measure(maximum_wire);
    write(&mut measure, decision, owner, ids)?;
    if measure.len().checked_add(2048).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut output = Writer::allocate(measure.len(), maximum_wire)?;
    write(&mut output, decision, owner, ids)?;
    output.finish()
}

pub(super) fn verify_parts(
    decision: D,
    owner: Option<&CreditOwner>,
    ids: &[Vec<u8>],
    encoded: &[u8],
    maximum_owned: usize,
) -> Result<(), E> {
    validate_owned(decision, owner, ids, maximum_owned)?;
    let mut writer = Writer::verify(encoded, encoded.len())?;
    write(&mut writer, decision, owner, ids)?;
    writer.finish_verification()
}

fn validate_owned(
    decision: D,
    owner: Option<&CreditOwner>,
    ids: &[Vec<u8>],
    maximum_owned: usize,
) -> Result<(), E> {
    if matches!(decision, D::Spent { .. }) {
        admission::ids(
            ids,
            u64::try_from(ids.len()).map_err(|_| E::LengthOverflow)?,
            CodecLimits::default(),
            maximum_owned,
        )?;
    } else if !ids.is_empty() {
        return Err(E::Malformed);
    }
    match decision {
        D::Granted { shard_id } | D::Spent { shard_id } | D::Renewed { shard_id } => {
            owner
                .ok_or(E::Malformed)?
                .lease(shard_id)
                .ok_or(E::Malformed)?;
        }
        D::Released | D::Owner => {
            owner.ok_or(E::Malformed)?;
        }
        D::Cleaned { pending, .. } => {
            let owner = owner.ok_or(E::Malformed)?.view();
            if (owner.cleanup_head <= owner.cleanup_tail) != pending {
                return Err(E::Malformed);
            }
        }
        D::Rejected(R::NotFound) if owner.is_some() => return Err(E::Malformed),
        D::Rejected(_) => {}
    }
    Ok(())
}

fn write(
    out: &mut Writer<'_>,
    decision: D,
    owner: Option<&CreditOwner>,
    ids: &[Vec<u8>],
) -> Result<(), E> {
    out.write(b"FWGR\0\x01")?;
    status::write(out, decision)?;
    out.optional(owner, |out, owner| {
        let mut measure = Writer::measure(900_000);
        owner::write(&mut measure, owner)?;
        out.u32(u32::try_from(measure.len()).map_err(|_| E::LengthOverflow)?)?;
        owner::write(out, owner)
    })?;
    out.u32(u32::try_from(ids.len()).map_err(|_| E::LengthOverflow)?)?;
    for id in ids {
        out.raw(id, 256)?;
    }
    Ok(())
}
