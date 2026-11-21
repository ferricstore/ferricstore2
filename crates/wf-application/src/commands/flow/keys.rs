//! Value reference names and digests as the Elixir server writes them.
//!
//! A reference is `f:{<tag>}:v:<kind>:...` where the tag of an auto bucket is
//! `fa:<bucket>` and that of a named partition is `f:` and the URL-safe
//! base64 of the first 16 bytes of the SHA-256 of its name (data format 2).
//! Clients treat references as opaque and hand them back to read values.

use sha2::{Digest, Sha256};

use crate::routing::{Partition, base64url_nopad};

fn tag(partition: &Partition) -> String {
    format!("{{{}}}", partition.tag())
}

/// The reference of a Flow's payload, result or error: `kind` is `p`, `r`
/// or `e`.
#[must_use]
pub fn value_ref(partition: &Partition, kind: char, id: &[u8], version: u64) -> Vec<u8> {
    let mut out = format!("f:{}:v:{kind}:", tag(partition)).into_bytes();
    out.extend_from_slice(id);
    out.extend_from_slice(format!(":{version}").as_bytes());
    out
}

/// The reference of a named value owned by Flow `owner`.
#[must_use]
pub fn named_value_ref(partition: &Partition, owner: &[u8], name: &[u8], version: u64) -> Vec<u8> {
    format!(
        "f:{}:v:n:{}:{}:{version}",
        tag(partition),
        base64url_nopad(owner),
        base64url_nopad(name)
    )
    .into_bytes()
}

/// The prefix of the keys Elixir writes a Flow's governance records under:
/// `f:{<tag>}:gov:<kind>:` (`e` effects, `li` the ledger index).
#[must_use]
pub fn governance_prefix(partition: &Partition, kind: &str) -> Vec<u8> {
    format!("f:{}:gov:{kind}:", tag(partition)).into_bytes()
}

/// The lowercase hex SHA-256 of a stored value.
#[must_use]
pub fn digest(bytes: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        for nibble in [byte >> 4, byte & 0x0F] {
            out.push(HEX.get(usize::from(nibble)).copied().unwrap_or(b'0'));
        }
    }
    out
}
