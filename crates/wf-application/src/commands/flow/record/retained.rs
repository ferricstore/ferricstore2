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

/// One kept value version, in the value codec's stored form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retained {
    /// The stored bytes.
    pub bytes: Vec<u8>,
}

impl FlowMeta {
    /// Keeps a replaced version; [`Self::prune_retained`] drops it once no
    /// kept history event can name it.
    pub fn retain(&mut self, (kind, name, version): (u8, &[u8], u64), bytes: &[u8]) {
        if version == 0 {
            return;
        }
        self.retained.insert(
            (kind, name.to_vec(), version),
            Retained {
                bytes: bytes.to_vec(),
            },
        );
    }

    /// Drops the kept versions no history event from version `oldest` on
    /// can name (audit gap G37): per kind and name, every version older
    /// than the newest one at or before `oldest`, which was current at the
    /// oldest event. A version an event names is never dropped while that
    /// event is kept, however many other values change.
    pub fn prune_retained(&mut self, oldest: u64) {
        let mut floors: std::collections::BTreeMap<(u8, Vec<u8>), u64> =
            std::collections::BTreeMap::new();
        for (kind, name, version) in self.retained.keys() {
            if *version <= oldest {
                let floor = floors.entry((*kind, name.clone())).or_insert(*version);
                *floor = (*floor).max(*version);
            }
        }
        self.retained.retain(|(kind, name, version), _| {
            floors
                .get(&(*kind, name.clone()))
                .is_none_or(|floor| version >= floor)
        });
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
