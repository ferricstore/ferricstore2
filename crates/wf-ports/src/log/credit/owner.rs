//! Neutral detached owner format; borrowed inspection precedes owned restoration.

use crate::log::{
    CodecError as E, CodecLimits,
    reader::{Reader, copy},
    writer::Writer,
};
use wf_domain::governance::credit::{
    CreditError, CreditLeaseData, CreditLimits, CreditOwner, CreditOwnerData, restore_credit_owner,
};
use wf_domain::workflow::{MAX_EXACT_MILLIS as MAX, control::PreparedDigest};

pub(super) struct ScannedCreditOwner<'a> {
    encoded: &'a [u8],
    scope: &'a [u8],
    fields: [u64; 6],
    fingerprint: Option<PreparedDigest>,
    count: usize,
    leases_at: usize,
    owned: usize,
    credit: CreditLimits,
}

impl ScannedCreditOwner<'_> {
    pub(super) const fn owned_bytes(&self) -> usize {
        self.owned
    }

    pub(super) const fn cleanup_pending(&self) -> bool {
        self.fields[4] <= self.fields[5]
    }

    pub(super) fn has_lease(&self, shard: u64) -> Result<bool, E> {
        let mut reader = Reader::new(&self.encoded[self.leases_at..], CodecLimits::default());
        for _ in 0..self.count {
            if read_lease(&mut reader, self.fields[2])?.shard_id == shard {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn into_owned(self, maximum_owned: usize) -> Result<CreditOwner, E> {
        if self.owned > maximum_owned {
            return Err(E::OwnedLimit);
        }
        let mut leases = Vec::new();
        leases
            .try_reserve_exact(self.count)
            .map_err(|_| E::Allocation)?;
        let mut reader = Reader::new(&self.encoded[self.leases_at..], CodecLimits::default());
        for _ in 0..self.count {
            leases.push(read_lease(&mut reader, self.fields[2])?);
        }
        reader.finish()?;
        restore_credit_owner(
            CreditOwnerData {
                scope: copy(self.scope)?,
                limit: self.fields[0],
                free: self.fields[1],
                epoch: self.fields[2],
                config_version: self.fields[3],
                policy_fingerprint: self.fingerprint,
                cleanup_head: self.fields[4],
                cleanup_tail: self.fields[5],
                leases,
            },
            self.credit,
        )
        .map_err(credit_error)
    }
}

pub(super) fn inspect(
    encoded: &[u8],
    credit: CreditLimits,
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<ScannedCreditOwner<'_>, E> {
    if encoded.len() > maximum_wire.min(900_000) {
        return Err(E::ByteLimit);
    }
    let mut reader = Reader::new(encoded, CodecLimits::default());
    if reader.take(4)? != b"FWGO" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    let scope = reader.nonempty(65_535)?;
    let mut fields = [0; 6];
    for field in &mut fields[..4] {
        *field = reader.u64()?;
    }
    let fingerprint = reader.optional(|reader| Ok(PreparedDigest::new(reader.fixed()?)))?;
    for field in &mut fields[4..] {
        *field = reader.u64()?;
    }
    if fields.iter().any(|n| *n > MAX) || fields[4] == 0 || fields[4] > fields[5] + 1 {
        return Err(E::Malformed);
    }
    let count = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
    if count > reader.remaining() / 61 {
        return Err(E::Malformed);
    }
    if count > credit.max_leases() {
        return Err(E::ItemLimit);
    }
    let owned = count
        .checked_mul(std::mem::size_of::<CreditLeaseData>())
        .and_then(|n| n.checked_add(scope.len()))
        .and_then(|n| n.checked_add(2048))
        .ok_or(E::LengthOverflow)?;
    if owned > maximum_owned.min(credit.max_owned_bytes()) {
        return Err(E::OwnedLimit);
    }
    let work = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4096))
        .and_then(|n| {
            count
                .checked_mul(256)
                .and_then(|slots| n.checked_add(slots))
        })
        .ok_or(E::LengthOverflow)?;
    if u64::try_from(work).map_err(|_| E::LengthOverflow)? > credit.max_work() {
        return Err(E::WorkLimit);
    }
    let leases_at = encoded.len() - reader.remaining();
    let mut previous = None;
    let mut allocated = 0_u64;
    for _ in 0..count {
        let lease = read_lease(&mut reader, fields[2])?;
        if previous.is_some_and(|shard| shard >= lease.shard_id) {
            return Err(E::Malformed);
        }
        allocated = allocated
            .checked_add(lease.available)
            .and_then(|n| n.checked_add(lease.in_use))
            .filter(|n| *n <= MAX)
            .ok_or(E::Malformed)?;
        previous = Some(lease.shard_id);
    }
    reader.finish()?;
    if fields[1] != fields[0].saturating_sub(allocated) {
        return Err(E::Malformed);
    }
    Ok(ScannedCreditOwner {
        encoded,
        scope,
        fields,
        fingerprint,
        count,
        leases_at,
        owned,
        credit,
    })
}

fn read_lease(reader: &mut Reader<'_>, owner_epoch: u64) -> Result<CreditLeaseData, E> {
    let lease = CreditLeaseData {
        shard_id: reader.u64()?,
        epoch: reader.u64()?,
        expires_at_ms: reader.u64()?,
        available: reader.u64()?,
        in_use: reader.u64()?,
        pending_reclaim: reader.u64()?,
        drain_rate_bits: reader.u64()?,
        last_spend_at_ms: reader.optional(Reader::u64)?,
        reservation_page: reader.u16()?,
        reservation_page_fill: reader.u16()?,
        reservations: Vec::new(),
    };
    let rate = f64::from_bits(lease.drain_rate_bits);
    if lease.shard_id > MAX
        || lease.epoch == 0
        || lease.epoch > owner_epoch
        || lease.expires_at_ms > MAX
        || lease.available > MAX
        || lease.in_use > MAX
        || lease.pending_reclaim > lease.available
        || !rate.is_finite()
        || rate < 0.0
        || rate > 9_007_199_254_740_991.0
        || lease.last_spend_at_ms.is_some_and(|time| time > MAX)
        || lease.reservation_page > 256
        || lease.reservation_page_fill > 256
        || (lease.reservation_page == 0) != (lease.reservation_page_fill == 0)
    {
        return Err(E::Malformed);
    }
    Ok(lease)
}

pub(super) fn encode(
    owner: &CreditOwner,
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<Vec<u8>, E> {
    let maximum = maximum_wire.min(900_000);
    let mut measure = Writer::measure(maximum);
    write(&mut measure, owner)?;
    if measure.len().checked_add(2048).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut output = Writer::allocate(measure.len(), maximum)?;
    write(&mut output, owner)?;
    output.finish()
}

pub(super) fn write(out: &mut Writer<'_>, owner: &CreditOwner) -> Result<(), E> {
    let data = owner.view();
    out.write(b"FWGO\0\x01")?;
    out.raw(&data.scope, 65_535)?;
    for n in [data.limit, data.free, data.epoch, data.config_version] {
        out.u64(n)?;
    }
    out.optional(data.policy_fingerprint.as_ref(), |out, digest| {
        out.write(digest.as_bytes())
    })?;
    out.u64(data.cleanup_head)?;
    out.u64(data.cleanup_tail)?;
    out.u32(u32::try_from(data.leases.len()).map_err(|_| E::LengthOverflow)?)?;
    for lease in &data.leases {
        if !lease.reservations.is_empty() {
            return Err(E::Malformed);
        }
        for n in [
            lease.shard_id,
            lease.epoch,
            lease.expires_at_ms,
            lease.available,
            lease.in_use,
            lease.pending_reclaim,
            lease.drain_rate_bits,
        ] {
            out.u64(n)?;
        }
        out.optional(lease.last_spend_at_ms.as_ref(), |out, time| out.u64(*time))?;
        out.u16(lease.reservation_page)?;
        out.u16(lease.reservation_page_fill)?;
    }
    Ok(())
}

fn credit_error(error: CreditError) -> E {
    match error {
        CreditError::Limit => E::OwnedLimit,
        CreditError::Allocation => E::Allocation,
        _ => E::Malformed,
    }
}
