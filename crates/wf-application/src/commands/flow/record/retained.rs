//! Replaced value versions a Flow keeps (ADR-016 phase C), so their
//! references still read, a rewind restores them and history shows them;
//! in memory, or on disk once a running Flow keeps too many (phase D).

use std::sync::Arc;

use super::FlowMeta;

/// A replaced payload in [`FlowMeta::retained`].
pub const RETAINED_PAYLOAD: u8 = b'p';
/// A replaced result.
pub const RETAINED_RESULT: u8 = b'r';
/// A replaced error.
pub const RETAINED_ERROR: u8 = b'e';
/// A replaced or dropped named value.
pub const RETAINED_NAMED: u8 = b'n';

/// A kept version's kind, name and version.
pub type RetainedKey = (u8, Vec<u8>, u64);

/// Kept versions by kind, name and version.
pub type RetainedMap = std::collections::BTreeMap<RetainedKey, Retained>;

/// One kept value version, in the value codec's stored form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Retained {
    /// The stored bytes, in memory.
    Bytes(Vec<u8>),
    /// Where the stored bytes are on disk (ADR-016 phase D).
    Stored(Arc<StoredValue>),
}

/// The files kept versions on disk are in.
pub trait ValueFiles: Send + Sync {
    /// One kept version in `file` is gone.
    fn release(&self, file: u64);
}

/// Where a kept version's stored bytes are on disk. It holds its file:
/// dropping it releases the hold.
pub struct StoredValue {
    /// The file.
    pub file: u64,
    /// Where its record starts in the file.
    pub offset: u64,
    /// The length of the stored bytes.
    pub length: u32,
    files: Arc<dyn ValueFiles>,
}

impl StoredValue {
    /// A version at `file`, `offset` and `length` whose hold on its file
    /// `files` already counts.
    #[must_use]
    pub fn new((file, offset, length): (u64, u64, u32), files: Arc<dyn ValueFiles>) -> Self {
        Self {
            file,
            offset,
            length,
            files,
        }
    }
}

impl Drop for StoredValue {
    fn drop(&mut self) {
        self.files.release(self.file);
    }
}

impl PartialEq for StoredValue {
    fn eq(&self, other: &Self) -> bool {
        (self.file, self.offset, self.length) == (other.file, other.offset, other.length)
    }
}

impl Eq for StoredValue {}

impl std::fmt::Debug for StoredValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoredValue")
            .field("file", &self.file)
            .field("offset", &self.offset)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl FlowMeta {
    /// Keeps a replaced version for as long as the Flow lives: Elixir keeps
    /// a replaced value readable after its events leave the history, until
    /// retention cleanup removes the Flow (audit gap G37). The version
    /// waits in [`FlowMeta::retained_unlogged`] for the next logged
    /// after-image.
    pub fn retain(&mut self, (kind, name, version): (u8, &[u8], u64), bytes: &[u8]) {
        if version == 0 {
            return;
        }
        let key = (kind, name.to_vec(), version);
        self.retained_unlogged.push(key.clone());
        self.retained.insert(key, Retained::Bytes(bytes.to_vec()));
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
