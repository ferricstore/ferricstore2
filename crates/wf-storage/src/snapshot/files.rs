//! Snapshot files of the queue node (ADR-013).
//!
//! Each snapshot is one file named by its id. Writing goes to a temp file,
//! syncs it, renames it into place and syncs the directory, so a crash leaves
//! either the whole snapshot or none. A file is the magic bytes, the id, the
//! body length, the body and a SHA-256 over everything before it. Reading
//! refuses a file whose checksum, length or id does not hold; the caller
//! decides whether an older snapshot may be used instead. Opening creates
//! missing directories and removes leftover temp files.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub mod group;

const MAGIC: &[u8; 8] = b"WFSNAP01";
const SUFFIX: &str = ".snapshot";
const TEMP_SUFFIX: &str = ".snapshot.tmp";
const DIGITS: usize = 20;
const HEADER_BYTES: usize = 24;
const DIGEST_BYTES: usize = 32;

/// A refused snapshot read, write or removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    /// The directory or a file could not be listed, read, written or removed.
    Unavailable,
    /// The snapshot with this id is damaged or names another id.
    Corrupt {
        /// The snapshot's id, from its file name.
        id: u64,
    },
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("snapshot storage unavailable"),
            Self::Corrupt { id } => write!(formatter, "snapshot {id} is corrupt"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// The snapshot directory.
#[derive(Clone, Debug)]
pub struct SnapshotFiles {
    directory: PathBuf,
}

fn unavailable(_: std::io::Error) -> SnapshotError {
    SnapshotError::Unavailable
}

fn sync_directory(directory: &Path) -> Result<(), SnapshotError> {
    File::open(directory)
        .and_then(|handle| handle.sync_all())
        .map_err(unavailable)
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Creates `directory` and any missing parents, syncing each parent after
/// its new entry.
fn create_durably(directory: &Path) -> Result<(), SnapshotError> {
    match fs::create_dir(directory) {
        Ok(()) => sync_directory(parent(directory)),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound && parent(directory) != directory => {
            create_durably(parent(directory))?;
            create_durably(directory)
        }
        Err(error) => Err(unavailable(error)),
    }
}

fn number(name: &str, suffix: &str) -> Option<u64> {
    let digits = name.strip_suffix(suffix)?;
    let all_digits = digits.len() == DIGITS && digits.bytes().all(|byte| byte.is_ascii_digit());
    all_digits.then(|| digits.parse().ok()).flatten()
}

impl SnapshotFiles {
    /// Uses `directory`, creating it and missing parents, and removes
    /// leftover temp files.
    ///
    /// # Errors
    /// [`SnapshotError::Unavailable`] when the directory cannot be created,
    /// listed or cleaned.
    pub fn open(directory: &Path) -> Result<Self, SnapshotError> {
        create_durably(directory)?;
        let files = Self {
            directory: directory.to_path_buf(),
        };
        for entry in fs::read_dir(directory).map_err(unavailable)? {
            let name = entry.map_err(unavailable)?.file_name();
            if name
                .to_str()
                .and_then(|name| number(name, TEMP_SUFFIX))
                .is_some()
            {
                fs::remove_file(directory.join(&name)).map_err(unavailable)?;
            }
        }
        Ok(files)
    }

    fn path(&self, id: u64, suffix: &str) -> PathBuf {
        self.directory.join(format!("{id:020}{suffix}"))
    }

    /// Snapshot ids present, oldest first.
    ///
    /// # Errors
    /// [`SnapshotError::Unavailable`] when the directory cannot be listed.
    pub fn ids(&self) -> Result<Vec<u64>, SnapshotError> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(unavailable)? {
            let name = entry.map_err(unavailable)?.file_name();
            if let Some(id) = name.to_str().and_then(|name| number(name, SUFFIX)) {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Writes snapshot `id` from `parts`, in order, as one body, and makes it
    /// durable before returning.
    ///
    /// # Errors
    /// [`SnapshotError::Unavailable`] when a write, sync or rename fails; no
    /// snapshot with `id` is then visible.
    pub fn write(&self, id: u64, parts: &[&[u8]]) -> Result<(), SnapshotError> {
        let length: usize = parts.iter().map(|part| part.len()).sum();
        let length = u64::try_from(length).map_err(|_| SnapshotError::Unavailable)?;
        let mut header = MAGIC.to_vec();
        header.extend_from_slice(&id.to_be_bytes());
        header.extend_from_slice(&length.to_be_bytes());
        let mut hasher = Sha256::new();
        let temp = self.path(id, TEMP_SUFFIX);
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)
            .map_err(unavailable)?;
        for part in std::iter::once(header.as_slice()).chain(parts.iter().copied()) {
            hasher.update(part);
            file.write_all(part).map_err(unavailable)?;
        }
        file.write_all(&hasher.finalize()).map_err(unavailable)?;
        file.sync_all().map_err(unavailable)?;
        drop(file);
        fs::rename(&temp, self.path(id, SUFFIX)).map_err(unavailable)?;
        sync_directory(&self.directory)
    }

    /// The newest snapshot's id and body, if any.
    ///
    /// # Errors
    /// [`SnapshotError::Corrupt`] when the newest snapshot is damaged;
    /// [`SnapshotError::Unavailable`] when it cannot be read.
    pub fn newest(&self) -> Result<Option<(u64, Vec<u8>)>, SnapshotError> {
        let Some(id) = self.ids()?.last().copied() else {
            return Ok(None);
        };
        let bytes = fs::read(self.path(id, SUFFIX)).map_err(unavailable)?;
        let body = check(&bytes, id).ok_or(SnapshotError::Corrupt { id })?;
        Ok(Some((id, body.to_vec())))
    }

    /// Removes every snapshot older than `id`, oldest first.
    ///
    /// # Errors
    /// [`SnapshotError::Unavailable`] when a removal fails; the rest stay.
    pub fn remove_before(&self, id: u64) -> Result<(), SnapshotError> {
        for older in self.ids()?.into_iter().filter(|older| *older < id) {
            fs::remove_file(self.path(older, SUFFIX)).map_err(unavailable)?;
        }
        sync_directory(&self.directory)
    }
}

/// The body of a whole, undamaged file for `id`.
fn check(bytes: &[u8], id: u64) -> Option<&[u8]> {
    let (signed, digest) = bytes.split_at_checked(bytes.len().checked_sub(DIGEST_BYTES)?)?;
    if Sha256::digest(signed).as_slice() != digest {
        return None;
    }
    let (header, body) = signed.split_at_checked(HEADER_BYTES)?;
    let (magic, numbers) = header.split_at_checked(MAGIC.len())?;
    let (named, length) = numbers.split_at_checked(8)?;
    let named = u64::from_be_bytes(named.try_into().ok()?);
    let length = u64::from_be_bytes(length.try_into().ok()?);
    let whole = magic == MAGIC && named == id && u64::try_from(body.len()).ok()? == length;
    whole.then_some(body)
}
