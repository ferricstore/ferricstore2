//! Partition naming and slot routing identical to the Elixir server
//! (ADR-011).
//!
//! A Flow created without a partition gets the auto bucket `crc32(id) & 0xFF`,
//! named `__flow_auto__:<bucket>`. A partition's hash tag is `fa:<bucket>`
//! for an auto bucket and `f:` plus the URL-safe base64 of the first 16 bytes
//! of `sha256(name)` otherwise (the data format 2 of a fresh Elixir node,
//! ADR-014); its slot is `crc32(tag) & 1023`, as for any key carrying that
//! tag between braces. Slots map to shards in contiguous ranges, the first
//! `1024 % shards` shards taking one extra slot.

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

    /// The hash tag of the partition's keys, without the braces.
    #[must_use]
    pub fn tag(&self) -> String {
        match self {
            Self::Auto(bucket) => format!("fa:{bucket}"),
            Self::Named(name) => {
                let digest = Sha256::digest(name);
                format!(
                    "f:{}",
                    base64url_nopad(digest.get(..16).unwrap_or_default())
                )
            }
        }
    }

    /// The routing slot, 0 through 1023.
    #[must_use]
    pub fn slot(&self) -> u16 {
        slot_for_tag(self.tag().as_bytes())
    }
}

/// The slot of a key whose hash tag is `tag`.
#[must_use]
pub fn slot_for_tag(tag: &[u8]) -> u16 {
    let [low, high, _, _] = (crc32(tag) & 0x3FF).to_le_bytes();
    u16::from_le_bytes([low, high])
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

/// CRC-32 (IEEE, as zlib and Erlang's `crc32/1`), hardware-accelerated
/// (ADR-015; the bit-by-bit loop it replaces was 2% of busy CPU).
fn crc32(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

/// Decodes base64 with the URL-safe alphabet and no padding; `None` for
/// other characters or a dangling sextet.
pub(crate) fn base64url_decode(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len().saturating_mul(3) / 4);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for character in text {
        let value = BASE64URL.iter().position(|known| known == character)?;
        buffer = buffer.wrapping_shl(6) | u32::try_from(value).ok()?;
        bits = bits.saturating_add(6);
        if bits >= 8 {
            bits = bits.saturating_sub(8);
            let [byte, ..] = buffer.wrapping_shr(bits).to_le_bytes();
            out.push(byte);
        }
    }
    (bits < 6).then_some(out)
}

/// Base64 with the URL-safe alphabet and no padding.
pub(crate) fn base64url_nopad(bytes: &[u8]) -> String {
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
