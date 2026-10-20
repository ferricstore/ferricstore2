//! Binary-only OTP28.3.1 release64LE fingerprints, without runtime identities.
//!
//! Adapted from the binary branch of `erl_term_hashing.c`, type11. Arithmetic and
//! byte order are explicit on both Linux and macOS; this is not an identity SHA.

use crate::policy::complete::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, charge_schema,
};

const C1: u64 = 0x87c3_7b91_1142_53d5;
const C2: u64 = 0x4cf5_ad43_2745_937f;

pub(in crate::policy::complete) fn binary_hash(
    bytes: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<u64, Error> {
    let length =
        u64::try_from(bytes.len()).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
    let bits = length
        .checked_mul(8)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    let fee = length
        .checked_add(128)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)?;
    let (mut alpha, mut beta, mut ticks) = (0_u64, 0_u64, 0_u64);
    mix_alpha(11, &mut alpha, beta, &mut ticks);
    mix_beta(bits, alpha, &mut beta, &mut ticks);
    let (chunks, tail) = bytes.as_chunks::<16>();
    for chunk in chunks {
        mix_alpha(little(&chunk[..8]), &mut alpha, beta, &mut ticks);
        mix_beta(little(&chunk[8..]), alpha, &mut beta, &mut ticks);
    }
    if tail.len() > 8 {
        beta ^= little(&tail[8..])
            .wrapping_mul(C2)
            .rotate_left(33)
            .wrapping_mul(C1);
    }
    if !tail.is_empty() {
        alpha ^= little(&tail[..tail.len().min(8)])
            .wrapping_mul(C1)
            .rotate_left(31)
            .wrapping_mul(C2);
    }
    alpha ^= ticks;
    beta ^= ticks;
    alpha = alpha.wrapping_add(beta);
    beta = beta.wrapping_add(alpha);
    alpha = finalize(alpha);
    beta = finalize(beta);
    alpha = alpha.wrapping_add(beta);
    beta = beta.wrapping_add(alpha);
    Ok(alpha ^ beta)
}

fn mix_alpha(value: u64, alpha: &mut u64, beta: u64, ticks: &mut u64) {
    let mixed = value.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
    *alpha = (*alpha ^ mixed)
        .rotate_left(27)
        .wrapping_add(beta)
        .wrapping_mul(5)
        .wrapping_add(0x52dc_e729);
    *ticks += 1;
}

fn mix_beta(value: u64, alpha: u64, beta: &mut u64, ticks: &mut u64) {
    let mixed = value.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
    *beta = (*beta ^ mixed)
        .rotate_left(31)
        .wrapping_add(alpha)
        .wrapping_mul(5)
        .wrapping_add(0x3849_5ab5);
    *ticks += 1;
}

fn little(bytes: &[u8]) -> u64 {
    bytes.iter().enumerate().fold(0, |value, (index, byte)| {
        value | (u64::from(*byte) << (index * 8))
    })
}

fn finalize(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^ (value >> 33)
}
