//! Partition naming and slot routing identical to the Elixir server
//! (ADR-011).
//!
//! A Flow created without a partition gets the auto bucket `crc32(id) & 0xFF`,
//! named `__flow_auto__:<bucket>`. A partition's routing input is
//! `fa:<bucket>` for an auto bucket and `f:<base64url(sha256(name))>`
//! otherwise; its slot is `crc32(input) & 1023`. Slots map to shards in
//! contiguous ranges, the first `1024 % shards` shards taking one extra slot.

use sha2::{Digest, Sha256};

/// Number of routing slots.
pub const SLOTS: u16 = 1024;
const AUTO_PREFIX: &[u8] = b"__flow_auto__:";
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Where a Flow lives.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Partition {
    /// A hidden auto bucket for Flows created without a partition.
    Auto(u8),
    /// A client-named partition.
    Named(Vec<u8>),
}

impl Partition {
    /// The partition of a new Flow: the explicit one, or its auto bucket.
    #[must_use]
    pub fn for_new_flow(id: &[u8], explicit: Option<&[u8]>) -> Self {
        explicit.map_or_else(
            || {
                let [bucket, ..] = crc32(id).to_le_bytes();
                Self::Auto(bucket)
            },
            Self::parse,
        )
    }

    /// Reads a client-supplied partition, recognising canonical auto bucket
    /// names (`__flow_auto__:` and 0 through 255 without leading zeros).
    #[must_use]
    pub fn parse(name: &[u8]) -> Self {
        name.strip_prefix(AUTO_PREFIX)
            .and_then(parse_bucket)
            .map_or_else(|| Self::Named(name.to_vec()), Self::Auto)
    }

    /// The name clients see.
    #[must_use]
    pub fn name(&self) -> Vec<u8> {
        match self {
            Self::Auto(bucket) => {
                let mut name = AUTO_PREFIX.to_vec();
                name.extend_from_slice(bucket.to_string().as_bytes());
                name
            }
            Self::Named(name) => name.clone(),
        }
    }

    /// The routing slot, 0 through 1023.
    #[must_use]
    pub fn slot(&self) -> u16 {
        let input = match self {
            Self::Auto(bucket) => format!("fa:{bucket}").into_bytes(),
            Self::Named(name) => {
                let mut input = b"f:".to_vec();
                input.extend_from_slice(base64url_nopad(&Sha256::digest(name)).as_bytes());
                input
            }
        };
        let [low, high, _, _] = (crc32(&input) & 0x3FF).to_le_bytes();
        u16::from_le_bytes([low, high])
    }
}

/// The shard owning `slot` when the slots are split over `shards` shards;
/// `None` for an out-of-range slot or a shard count of 0 or above 1024.
#[must_use]
pub fn shard_for_slot(slot: u16, shards: u16) -> Option<u16> {
    if shards == 0 || shards > SLOTS || slot >= SLOTS {
        return None;
    }
    let per = SLOTS.checked_div(shards)?;
    let extra = SLOTS.checked_rem(shards)?;
    let wide_width = per.checked_add(1)?;
    let wide = extra.checked_mul(wide_width)?;
    if slot < wide {
        slot.checked_div(wide_width)
    } else {
        extra.checked_add(slot.checked_sub(wide)?.checked_div(per)?)
    }
}

/// Canonical decimal 0 through 255 without leading zeros.
fn parse_bucket(digits: &[u8]) -> Option<u8> {
    if digits.is_empty() || digits.len() > 3 || (digits.len() > 1 && digits.first() == Some(&b'0'))
    {
        return None;
    }
    digits.iter().try_fold(0_u8, |value, digit| {
        let digit = digit.is_ascii_digit().then(|| digit.wrapping_sub(b'0'))?;
        value.checked_mul(10)?.checked_add(digit)
    })
}

/// CRC-32 (IEEE, as zlib), computed bit by bit.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = crc.wrapping_shr(1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Base64 with the URL-safe alphabet and no padding.
fn base64url_nopad(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let (first, second, third) = match *chunk {
            [first, second, third] => (first, Some(second), Some(third)),
            [first, second] => (first, Some(second), None),
            [first] => (first, None, None),
            _ => continue,
        };
        let group = (u32::from(first).wrapping_shl(16))
            | (u32::from(second.unwrap_or(0)).wrapping_shl(8))
            | u32::from(third.unwrap_or(0));
        let characters = match (second, third) {
            (Some(_), Some(_)) => 4,
            (Some(_), None) => 3,
            _ => 2,
        };
        for shift in [18_u32, 12, 6, 0].into_iter().take(characters) {
            let index = usize::try_from(group.wrapping_shr(shift) & 0x3F).unwrap_or(0);
            if let Some(character) = BASE64URL.get(index) {
                out.push(char::from(*character));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
