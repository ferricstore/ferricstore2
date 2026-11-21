//! Page cursors: `fqc1_` and the URL-safe base64 of a version, an expiry, a
//! digest of the query they continue, the continuation, and an HMAC-SHA256
//! tag under the node's key. As Elixir's cursors, they bind to the query,
//! expire after five minutes and are opaque; this node authenticates rather
//! than encrypts them, so the continuation (the last row's sort key) is
//! readable but cannot be altered.

use sha2::{Digest, Sha256};

use super::error::Reason;
use crate::routing::{base64url_decode, base64url_nopad};

const PREFIX: &[u8] = b"fqc1_";
const VERSION: u8 = 1;
/// Elixir's default cursor lifetime.
const TTL_MS: u64 = 5 * 60 * 1_000;
const DIGEST_BYTES: usize = 16;
const TAG_BYTES: usize = 16;
const BLOCK: usize = 64;

/// The node's cursor key.
pub type Key = [u8; 32];

/// HMAC-SHA256 of `message` under `key`, also keying schedule ownership digests.
pub(crate) fn hmac(key: &Key, message: &[u8]) -> [u8; 32] {
    let mut inner = [0x36_u8; BLOCK];
    let mut outer = [0x5c_u8; BLOCK];
    for ((inner, outer), byte) in inner.iter_mut().zip(outer.iter_mut()).zip(key) {
        *inner ^= byte;
        *outer ^= byte;
    }
    let inner_hash = Sha256::new()
        .chain_update(inner)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer)
        .chain_update(inner_hash)
        .finalize()
        .into()
}

fn tag(key: &Key, payload: &[u8]) -> Vec<u8> {
    hmac(key, payload)
        .get(..TAG_BYTES)
        .unwrap_or_default()
        .to_vec()
}

/// A cursor continuing the query with `digest` after `continuation`.
#[must_use]
pub fn issue(key: &Key, digest: &[u8; 32], now_ms: u64, continuation: &[u8]) -> Vec<u8> {
    let mut payload = vec![VERSION];
    payload.extend_from_slice(&now_ms.saturating_add(TTL_MS).to_be_bytes());
    payload.extend_from_slice(digest.get(..DIGEST_BYTES).unwrap_or_default());
    payload.extend_from_slice(continuation);
    let tag = tag(key, &payload);
    payload.extend_from_slice(&tag);
    let mut token = PREFIX.to_vec();
    token.extend_from_slice(base64url_nopad(&payload).as_bytes());
    token
}

/// The continuation of a cursor this node issued for the query with
/// `digest`.
///
/// # Errors
/// `query_cursor_invalid` for a cursor of another query, node or format,
/// and `query_cursor_expired` past its lifetime.
pub fn open(key: &Key, digest: &[u8; 32], now_ms: u64, token: &[u8]) -> Result<Vec<u8>, Reason> {
    let encoded = token
        .strip_prefix(PREFIX)
        .ok_or(Reason::QueryCursorInvalid)?;
    let raw = base64url_decode(encoded).ok_or(Reason::QueryCursorInvalid)?;
    let split = raw
        .len()
        .checked_sub(TAG_BYTES)
        .ok_or(Reason::QueryCursorInvalid)?;
    let (payload, sent) = raw
        .split_at_checked(split)
        .ok_or(Reason::QueryCursorInvalid)?;
    let expected = tag(key, payload);
    // Every byte is compared, so the time taken does not reveal the tag.
    let same = sent.len() == expected.len()
        && sent
            .iter()
            .zip(&expected)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0;
    if !same {
        return Err(Reason::QueryCursorInvalid);
    }
    let (version, rest) = payload.split_first().ok_or(Reason::QueryCursorInvalid)?;
    let (expires, rest) = rest.split_at_checked(8).ok_or(Reason::QueryCursorInvalid)?;
    let (bound_to, continuation) = rest
        .split_at_checked(DIGEST_BYTES)
        .ok_or(Reason::QueryCursorInvalid)?;
    if *version != VERSION || Some(bound_to) != digest.get(..DIGEST_BYTES) {
        return Err(Reason::QueryCursorInvalid);
    }
    let expires = u64::from_be_bytes(expires.try_into().map_err(|_| Reason::QueryCursorInvalid)?);
    if now_ms >= expires {
        return Err(Reason::QueryCursorExpired);
    }
    Ok(continuation.to_vec())
}

#[cfg(test)]
mod tests {
    use super::{Reason, issue, open};

    #[test]
    fn a_cursor_opens_for_its_own_query_until_it_expires() {
        let key = [7_u8; 32];
        let digest = [1_u8; 32];
        let token = issue(&key, &digest, 1_000, b"after");
        assert!(token.starts_with(b"fqc1_"));
        assert_eq!(open(&key, &digest, 2_000, &token), Ok(b"after".to_vec()));
        assert_eq!(
            open(&key, &[2_u8; 32], 2_000, &token),
            Err(Reason::QueryCursorInvalid)
        );
        assert_eq!(
            open(&[8_u8; 32], &digest, 2_000, &token),
            Err(Reason::QueryCursorInvalid)
        );
        assert_eq!(
            open(&key, &digest, 301_000, &token),
            Err(Reason::QueryCursorExpired)
        );
        let mut altered = token.clone();
        if let Some(last) = altered.last_mut() {
            *last = if *last == b'A' { b'B' } else { b'A' };
        }
        assert_eq!(
            open(&key, &digest, 2_000, &altered),
            Err(Reason::QueryCursorInvalid)
        );
    }
}
