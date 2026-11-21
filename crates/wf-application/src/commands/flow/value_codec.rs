//! How a Flow stores a client's value (audit gap G5).
//!
//! Elixir keeps any term as a payload, result, error, cancel reason or
//! named value. This node stores a binary as it is and any other term as
//! its term encoding behind a marker, so every stored value reads back as
//! the client sent it with no flag beside it. A binary that begins like the
//! marker is escaped, as Elixir's value codec escapes magic-prefixed
//! binaries.

use super::term::Term;

/// Starts a value stored as a term.
const TERM: [u8; 3] = [0xF7, b'F', b'T'];
/// Starts a binary that itself began like a marker.
const ESCAPED: [u8; 3] = [0xF7, b'F', b'B'];

/// Whether a binary begins like a marker and must be escaped.
fn marked(bytes: &[u8]) -> bool {
    bytes.starts_with(&TERM[..2])
}

/// The stored form of a client's value; `None` when the term cannot be
/// encoded.
#[must_use]
pub fn store(value: &Term) -> Option<Vec<u8>> {
    match value {
        Term::Bin(bytes) => Some(escape(bytes.clone())),
        term => {
            let mut out = TERM.to_vec();
            term.encode(&mut out)?;
            Some(out)
        }
    }
}

/// The stored form of a binary: itself, escaped when it begins like a
/// marker. Queue fast-path payloads and results pass through this.
#[must_use]
pub fn escape(bytes: Vec<u8>) -> Vec<u8> {
    if marked(&bytes) {
        let mut out = ESCAPED.to_vec();
        out.extend_from_slice(&bytes);
        out
    } else {
        bytes
    }
}

/// The client's value from its stored form.
#[must_use]
pub fn load(stored: &[u8]) -> Term {
    if let Some(encoded) = stored.strip_prefix(&TERM)
        && let Some((term, [])) = Term::decode(encoded)
    {
        return term;
    }
    match stored.strip_prefix(&ESCAPED) {
        Some(bytes) => Term::Bin(bytes.to_vec()),
        None => Term::Bin(stored.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::{escape, load, store};
    use crate::commands::flow::term::Term;

    #[test]
    fn every_value_reads_back_as_sent() {
        let map = Term::Map(vec![(b"n".to_vec(), Term::Int(1))]);
        for value in [
            Term::text("plain"),
            Term::Bin(Vec::new()),
            Term::Bin(vec![0xF7, b'F', b'T', 1, 2]),
            Term::Bin(vec![0xF7, b'F']),
            Term::Bin(vec![0xF7]),
            Term::List(vec![Term::text("a"), Term::Int(2)]),
            map,
            Term::Bool(true),
        ] {
            let stored = store(&value).unwrap_or_default();
            assert_eq!(load(&stored), value, "{stored:?}");
        }
    }

    #[test]
    fn plain_binaries_are_stored_as_they_are() {
        assert_eq!(escape(b"payload".to_vec()), b"payload");
        assert_eq!(store(&Term::text("payload")), Some(b"payload".to_vec()));
    }
}
