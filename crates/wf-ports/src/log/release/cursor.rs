//! Bounded flat cursor ownership; source text is interpreted without UTF-8 copies.
use super::E;
use crate::log::writer::Writer;
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;
/// Validated mathematical cursor; it is not a reusable read or retention permit.
pub struct ReleaseReconcileCursor {
    next: u64,
    shards: u64,
    positions: Vec<(u64, u64)>,
}
impl ReleaseReconcileCursor {
    /// Admits bounds before allocating and validating ordered descriptors.
    /// # Errors
    /// Refuses numeric/duplicate/count data, prospective owner bounds or allocation.
    pub fn from_parts(
        next: u64,
        positions: &[(u64, u64)],
        shards: u64,
        maximum_owned: usize,
    ) -> Result<Self, E> {
        header(next, shards)?;
        admit(positions.len(), shards, maximum_owned)?;
        if positions
            .iter()
            .any(|(shard, sequence)| *shard >= shards || *sequence == 0 || *sequence > MAX)
        {
            return Err(E::Malformed);
        }
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(positions.len())
            .map_err(|_| E::Allocation)?;
        owned.extend_from_slice(positions);
        finish(next, shards, owned)
    }
    /// Original next selected source shard.
    #[must_use]
    pub const fn next_shard(&self) -> u64 {
        self.next
    }
    /// Configured original source count, without placement authority.
    #[must_use]
    pub const fn shard_count(&self) -> u64 {
        self.shards
    }
    /// Canonical ascending per-source positions, without a copied map.
    #[must_use]
    pub fn positions(&self) -> &[(u64, u64)] {
        &self.positions
    }
}
fn header(next: u64, shards: u64) -> Result<(), E> {
    if shards == 0 || next >= shards {
        Err(E::Malformed)
    } else {
        Ok(())
    }
}
fn admit(count: usize, shards: u64, maximum_owned: usize) -> Result<(), E> {
    if u64::try_from(count).map_err(|_| E::LengthOverflow)? > shards {
        return Err(E::Malformed);
    }
    let required = count
        .checked_mul(std::mem::size_of::<(u64, u64)>())
        .and_then(|n| n.checked_add(128))
        .ok_or(E::LengthOverflow)?;
    if required > maximum_owned {
        Err(E::OwnedLimit)
    } else {
        Ok(())
    }
}
fn finish(
    next: u64,
    shards: u64,
    mut positions: Vec<(u64, u64)>,
) -> Result<ReleaseReconcileCursor, E> {
    positions.sort_unstable_by_key(|entry| entry.0);
    if positions.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(E::Malformed);
    }
    Ok(ReleaseReconcileCursor {
        next,
        shards,
        positions,
    })
}
pub(super) fn decode(
    encoded: Option<&[u8]>,
    shards: u64,
    maximum_owned: usize,
) -> Result<ReleaseReconcileCursor, E> {
    if shards == 0 {
        return Err(E::Malformed);
    }
    let Some(encoded) = encoded else {
        return ReleaseReconcileCursor::from_parts(0, &[], shards, maximum_owned);
    };
    // u128 covers the source formula even when the configured count is u64::MAX.
    if encoded.len() as u128 > 128 + u128::from(shards) * 96 {
        return Err(E::ByteLimit);
    }
    let body = encoded.strip_prefix(b"v1:").ok_or(E::Malformed)?;
    let separator = body
        .iter()
        .position(|byte| *byte == b'|')
        .ok_or(E::Malformed)?;
    let next = integer(&body[..separator])?;
    header(next, shards)?;
    let text = &body[separator + 1..];
    let count = positions(text, shards, |_| Ok(()))?;
    admit(count, shards, maximum_owned)?;
    let mut owned = Vec::new();
    owned.try_reserve_exact(count).map_err(|_| E::Allocation)?;
    positions(text, shards, |entry| {
        owned.push(entry);
        Ok(())
    })?;
    finish(next, shards, owned)
}
fn positions(
    encoded: &[u8],
    shards: u64,
    mut visit: impl FnMut((u64, u64)) -> Result<(), E>,
) -> Result<usize, E> {
    if encoded.is_empty() {
        return Ok(0);
    }
    let mut count = 0_usize;
    for entry in encoded.split(|byte| *byte == b',') {
        let equal = entry
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or(E::Malformed)?;
        let shard = integer(&entry[..equal])?;
        let sequence = integer(&entry[equal + 1..])?;
        if shard >= shards || sequence == 0 || sequence > MAX {
            return Err(E::Malformed);
        }
        count = count.checked_add(1).ok_or(E::LengthOverflow)?;
        if u64::try_from(count).map_err(|_| E::LengthOverflow)? > shards {
            return Err(E::Malformed);
        }
        visit((shard, sequence))?;
    }
    Ok(count)
}
fn integer(encoded: &[u8]) -> Result<u64, E> {
    let (negative, digits) = match encoded.first() {
        Some(b'+') => (false, &encoded[1..]),
        Some(b'-') => (true, &encoded[1..]),
        _ => (false, encoded),
    };
    if digits.is_empty() {
        return Err(E::Malformed);
    }
    let mut number = 0_u64;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return Err(E::Malformed);
        }
        number = number
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(byte - b'0')))
            .ok_or(E::Malformed)?;
    }
    if negative && number != 0 {
        Err(E::Malformed)
    } else {
        Ok(number)
    }
}
pub(super) fn encode(cursor: &ReleaseReconcileCursor, maximum: usize) -> Result<Vec<u8>, E> {
    let mut measure = Writer::measure(maximum);
    write(cursor, &mut measure)?;
    let mut output = Writer::allocate(measure.len(), maximum)?;
    write(cursor, &mut output)?;
    output.finish()
}
fn write(cursor: &ReleaseReconcileCursor, output: &mut Writer<'_>) -> Result<(), E> {
    let mut digits = [0_u8; 20];
    output.write(b"v1:")?;
    output.write(decimal(cursor.next, &mut digits)?)?;
    output.write(b"|")?;
    for (index, (shard, sequence)) in cursor.positions.iter().enumerate() {
        if index != 0 {
            output.write(b",")?;
        }
        output.write(decimal(*shard, &mut digits)?)?;
        output.write(b"=")?;
        output.write(decimal(*sequence, &mut digits)?)?;
    }
    Ok(())
}
fn decimal(mut number: u64, output: &mut [u8; 20]) -> Result<&[u8], E> {
    let mut at = output.len();
    loop {
        at = at.checked_sub(1).ok_or(E::LengthOverflow)?;
        output[at] = b'0' + u8::try_from(number % 10).map_err(|_| E::LengthOverflow)?;
        number /= 10;
        if number == 0 {
            return Ok(&output[at..]);
        }
    }
}
