//! Replaced value versions a Flow keeps (ADR-016 phase C), so their
//! references still read, a rewind restores them and history shows them.

use super::FlowMeta;

/// A replaced payload in [`FlowMeta::retained`].
pub const RETAINED_PAYLOAD: u8 = b'p';
/// A replaced result.
pub const RETAINED_RESULT: u8 = b'r';
/// A replaced error.
pub const RETAINED_ERROR: u8 = b'e';
/// A replaced or dropped named value.
pub const RETAINED_NAMED: u8 = b'n';

/// Kept versions by kind, name and version.
pub type RetainedMap = std::collections::BTreeMap<(u8, Vec<u8>, u64), Retained>;

/// One kept value version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retained {
    /// The stored bytes.
    pub bytes: Vec<u8>,
    /// Whether the bytes are this node's term encoding.
    pub term: bool,
}

impl FlowMeta {
    /// Keeps a replaced version; the oldest go once more are kept than the
    /// history holds events, since no event can name them then.
    pub fn retain(&mut self, (kind, name, version): (u8, &[u8], u64), bytes: &[u8], term: bool) {
        if version == 0 {
            return;
        }
        self.retained.insert(
            (kind, name.to_vec(), version),
            Retained {
                bytes: bytes.to_vec(),
                term,
            },
        );
        let cap = usize::try_from(self.history_max_events.max(1)).unwrap_or(usize::MAX);
        while self.retained.len() > cap {
            let Some(oldest) = self
                .retained
                .keys()
                .min_by_key(|(_, _, version)| *version)
                .cloned()
            else {
                break;
            };
            self.retained.remove(&oldest);
        }
    }

    /// A kept version, if any.
    #[must_use]
    pub fn retained(&self, kind: u8, name: &[u8], version: u64) -> Option<&Retained> {
        self.retained.get(&(kind, name.to_vec(), version))
    }

    /// The newest version a named value has had, kept or current.
    #[must_use]
    pub fn newest_named_version(&self, name: &[u8]) -> u64 {
        let kept = self
            .retained
            .keys()
            .filter(|(kind, kept, _)| *kind == RETAINED_NAMED && kept.as_slice() == name)
            .map(|(_, _, version)| *version)
            .max()
            .unwrap_or(0);
        self.values
            .get(name)
            .map_or(kept, |value| value.version.max(kept))
    }
}
