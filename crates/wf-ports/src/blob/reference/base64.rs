//! Canonical unpadded URL Base64 without decoded or intermediate owners.

use crate::blob::OriginalValueBuildError as E;

const BASE64_URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(in crate::blob::reference) fn encoded_len(length: usize) -> Result<usize, E> {
    let full = (length / 3).checked_mul(4).ok_or(E::Limit)?;
    let tail = match length % 3 {
        0 => 0,
        1 => 2,
        _ => 3,
    };
    full.checked_add(tail).ok_or(E::Limit)
}

pub(in crate::blob::reference) fn valid(input: &[u8]) -> bool {
    if input.len() % 4 == 1 || input.iter().any(|byte| digit(*byte).is_none()) {
        return false;
    }
    match input.len() % 4 {
        2 => input
            .last()
            .and_then(|byte| digit(*byte))
            .is_some_and(|value| value.trailing_zeros() >= 4),
        3 => input
            .last()
            .and_then(|byte| digit(*byte))
            .is_some_and(|value| value.trailing_zeros() >= 2),
        _ => true,
    }
}

pub(in crate::blob::reference) fn encode_into(input: &[u8], output: &mut [u8]) -> Result<usize, E> {
    let length = encoded_len(input.len())?;
    let target = output.get_mut(..length).ok_or(E::Limit)?;
    let mut cursor = 0_usize;
    for chunk in input.chunks(3) {
        let first = *chunk.first().ok_or(E::InvalidInput)?;
        let second = match chunk.get(1) {
            Some(byte) => *byte,
            None => 0,
        };
        let third = match chunk.get(2) {
            Some(byte) => *byte,
            None => 0,
        };
        let values = [
            first >> 2,
            ((first & 3) << 4) | (second >> 4),
            ((second & 15) << 2) | (third >> 6),
            third & 63,
        ];
        let count = if chunk.len() == 3 { 4 } else { chunk.len() + 1 };
        for value in values.into_iter().take(count) {
            let byte = *BASE64_URL_ALPHABET
                .get(usize::from(value))
                .ok_or(E::InvalidInput)?;
            *target.get_mut(cursor).ok_or(E::Limit)? = byte;
            cursor = cursor.checked_add(1).ok_or(E::Limit)?;
        }
    }
    Ok(cursor)
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}
