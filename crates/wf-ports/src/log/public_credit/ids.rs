//! One exact source batch, with prospective admission before the first allocation.
use super::*;
pub(super) fn generate(
    amount: u64,
    spend: CreditSpendPreparation,
    maximum_owned: usize,
) -> Result<Vec<Vec<u8>>, E> {
    if !(1..=1000).contains(&amount) || spend.lease_epoch > wf_domain::workflow::MAX_EXACT_MILLIS {
        return Err(E::Malformed);
    }
    let count = usize::try_from(amount).map_err(|_| E::LengthOverflow)?;
    let batch = base64(spend.batch);
    let mut digits = [0; 20];
    let epoch = decimal(spend.lease_epoch, &mut digits)?;
    let prefix = 5 + epoch.len() + 1 + batch.len() + 1;
    let mut owned = 128_usize
        .checked_add(
            count
                .checked_mul(std::mem::size_of::<Vec<u8>>())
                .ok_or(E::LengthOverflow)?,
        )
        .ok_or(E::LengthOverflow)?;
    let mut ordinal = [0; 20];
    for n in 1..=amount {
        owned = owned
            .checked_add(prefix)
            .and_then(|v| v.checked_add(decimal(n, &mut ordinal).ok()?.len()))
            .ok_or(E::LengthOverflow)?;
    }
    if owned > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut ids = Vec::new();
    ids.try_reserve_exact(count).map_err(|_| E::Allocation)?;
    for n in 1..=amount {
        let number = decimal(n, &mut ordinal)?;
        let mut id = Vec::new();
        id.try_reserve_exact(prefix + number.len())
            .map_err(|_| E::Allocation)?;
        for part in [b"flr1:".as_slice(), epoch, b":", &batch, b":", number] {
            id.extend_from_slice(part);
        }
        ids.push(id);
    }
    Ok(ids)
}
fn decimal(mut n: u64, out: &mut [u8; 20]) -> Result<&[u8], E> {
    let mut at = out.len();
    loop {
        at = at.checked_sub(1).ok_or(E::LengthOverflow)?;
        out[at] = b'0' + u8::try_from(n % 10).map_err(|_| E::LengthOverflow)?;
        n /= 10;
        if n == 0 {
            return Ok(&out[at..]);
        }
    }
}
fn base64(bytes: [u8; 16]) -> [u8; 22] {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = [0; 22];
    for (input, output) in bytes[..15]
        .as_chunks::<3>()
        .0
        .iter()
        .zip(out[..20].as_chunks_mut::<4>().0)
    {
        output[0] = ABC[usize::from(input[0] >> 2)];
        output[1] = ABC[usize::from(((input[0] & 3) << 4) | (input[1] >> 4))];
        output[2] = ABC[usize::from(((input[1] & 15) << 2) | (input[2] >> 6))];
        output[3] = ABC[usize::from(input[2] & 63)];
    }
    out[20] = ABC[usize::from(bytes[15] >> 2)];
    out[21] = ABC[usize::from((bytes[15] & 3) << 4)];
    out
}
