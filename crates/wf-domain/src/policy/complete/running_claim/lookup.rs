//! Charged logarithmic binary-key lookup, including unaligned bitstring peers.

use super::super::flow_input::value::PolicyInputKind as Kind;
use super::super::{
    PolicySchemaCharge,
    original_entry::{TermPosition, indexed::OriginalPolicyCurrentIndex as Index},
};
use super::{Error, overflow, pay};
use std::cmp::Ordering;

pub(super) fn binary(
    index: &Index<'_, '_>,
    map: TermPosition,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<TermPosition>, Error> {
    let map = index.at(map);
    let (mut low, mut high) = (0, map.len());
    while low < high {
        let middle = low + (high - low) / 2;
        pay(charge, 128)?;
        let (key, value) = map.row(middle).ok_or(Error::Malformed)?;
        // The auxiliary identity index orders atom/integer/float before binary,
        // then list/tuple/map/runtime classes. This is not OTP map iteration order.
        let order = if let Some((bytes, bits)) = key.bitstring() {
            pay(
                charge,
                bytes
                    .len()
                    .checked_add(name.len())
                    .and_then(|n| n.checked_add(128))
                    .ok_or_else(overflow)?,
            )?;
            compare_bits(bytes, bits, name)?
        } else if key.atom_bytes().is_some() || key.kind() == Kind::Integer || key.float().is_some()
        {
            Ordering::Less
        } else {
            Ordering::Greater
        };
        match order {
            Ordering::Equal => return Ok(Some(value.position())),
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
        }
    }
    Ok(None)
}

fn compare_bits(bytes: &[u8], bits: usize, name: &[u8]) -> Result<Ordering, Error> {
    let name_bits = name.len().checked_mul(8).ok_or_else(overflow)?;
    let shared = bits.min(name_bits);
    let full = shared / 8;
    let order = bytes[..full].cmp(&name[..full]);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let remainder = shared % 8;
    if remainder != 0 {
        let order = (bytes[full] >> (8 - remainder)).cmp(&(name[full] >> (8 - remainder)));
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(bits.cmp(&name_bits))
}
