//! Portable `phash2` for the source's binary/u64/u64 retry tuple.
//!
//! Fixed-shape adaptation of OTP-29.0.5 `erl_term_hashing.c` `make_hash2` and
//! `block_hash` (Ericsson AB, Copyright 2022-2026, Apache-2.0):
//! [upstream source](https://github.com/erlang/otp/blob/OTP-29.0.5/erts/emulator/beam/erl_term_hashing.c).
//! No VM representation, generic term stack or caller-owned binary copy is used.

use super::JitterSample;

const GOLDEN: u32 = 0x9e37_79b9;

/// Computes the full source binary hash before the caller's explicit range.
#[must_use]
pub fn source_binary_hash(bytes: &[u8]) -> u32 {
    binary(bytes, 0)
}

/// Computes source `phash2({binary_id, next_attempt, now})` in its default range.
///
/// This mathematical helper allocates nothing and uses wrapping 32-bit mixing
/// and little-endian binary words, independently of host byte order. It does not
/// generate time, read policy or prove that the caller passed Fresh guards.
#[must_use]
pub fn source_retry_jitter(id: &[u8], attempt: u64, now: u64) -> JitterSample {
    let hash = pair(3, 0, 0x8ff3_4781, 0);
    let hash = binary(id, hash);
    let hash = integer(attempt, hash);
    JitterSample::new(integer(now, hash) & 0x07ff_ffff)
}

fn integer(value: u64, hash: u32) -> u32 {
    let bytes = value.to_le_bytes();
    let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let high = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    // OTP intentionally uses the signed-small-28 boundary even on 64-bit VMs.
    let constant = if value < (1 << 27) {
        GOLDEN
    } else {
        0xcc62_3af3
    };
    pair(low, high, constant, hash)
}

fn pair(first: u32, second: u32, constant: u32, hash: u32) -> u32 {
    mix(
        constant.wrapping_add(first),
        constant.wrapping_add(second),
        hash,
    )
    .2
}

fn binary(bytes: &[u8], hash: u32) -> u32 {
    let initial = hash.wrapping_add(0x08d1_2e65);
    // OTP preserves its historical low-32-bit byte length behavior.
    let count = bytes.len() & 0xffff_ffff;
    if count == 0 {
        return initial;
    }
    let length = count.to_le_bytes();
    let length = u32::from_le_bytes([length[0], length[1], length[2], length[3]]);
    let (chunks, remainder) = bytes[..count].as_chunks::<12>();
    let (mut a, mut b, mut c) = (GOLDEN, GOLDEN, initial);
    for chunk in chunks {
        (a, b, c) = mix(
            a.wrapping_add(word(&chunk[..4])),
            b.wrapping_add(word(&chunk[4..8])),
            c.wrapping_add(word(&chunk[8..12])),
        );
    }
    let mut tail = [0_u8; 12];
    tail[..remainder.len()].copy_from_slice(remainder);
    a = a.wrapping_add(word(&tail[..4]));
    b = b.wrapping_add(word(&tail[4..8]));
    c = c
        .wrapping_add(length)
        .wrapping_add(u32::from(tail[8]) << 8)
        .wrapping_add(u32::from(tail[9]) << 16)
        .wrapping_add(u32::from(tail[10]) << 24);
    mix(a, b, c).2
}

fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 13);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 8);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 13);
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 12);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 16);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 5);
    a = a.wrapping_sub(b).wrapping_sub(c) ^ (c >> 3);
    b = b.wrapping_sub(c).wrapping_sub(a) ^ (a << 10);
    c = c.wrapping_sub(a).wrapping_sub(b) ^ (b >> 15);
    (a, b, c)
}
