//! Exact source-derived scope bytes, without policy or ownership authority.
use sha2::{Digest, Sha256};

/// Computes the pinned source's running-credit scope without heap ownership.
#[must_use]
pub fn running_claim_scope(workflow_type: &[u8]) -> [u8; 56] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let digest = Sha256::digest(workflow_type);
    let mut result = [0; 56];
    result[..13].copy_from_slice(b"flow-running:");
    for (chunk, output) in digest[..30]
        .as_chunks::<3>()
        .0
        .iter()
        .zip(result[13..53].as_chunks_mut::<4>().0)
    {
        output[0] = ALPHABET[usize::from(chunk[0] >> 2)];
        output[1] = ALPHABET[usize::from((chunk[0] & 3) << 4 | chunk[1] >> 4)];
        output[2] = ALPHABET[usize::from((chunk[1] & 15) << 2 | chunk[2] >> 6)];
        output[3] = ALPHABET[usize::from(chunk[2] & 63)];
    }
    result[53] = ALPHABET[usize::from(digest[30] >> 2)];
    result[54] = ALPHABET[usize::from((digest[30] & 3) << 4 | digest[31] >> 4)];
    result[55] = ALPHABET[usize::from((digest[31] & 15) << 2)];
    result
}
