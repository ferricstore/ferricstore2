//! Cold files for finished Flows (ADR-016).
//!
//! A shard writes each finished Flow's image to a cold file and keeps only
//! where it is. Files are append-only and written by one [`ColdWriter`]
//! each; after a restart they are only read, never appended to. A record is
//! a `u32` length, the CRC32 of the image and the image, big-endian. File
//! ids are node-wide, so a location keeps working when the shard count
//! changes. Each file counts the stubs that point into it; once its writer
//! is sealed and the count reaches zero, the file is unused.
//!
//! Durability: [`ColdWriter::sync`] makes appended records durable. A
//! snapshot syncs its shards' writers first, so a stub in a snapshot names
//! durable bytes; anything written later is written again by replay. Replay
//! reads the files the last snapshot names, so a store that keeps files for
//! snapshots ([`ColdStore::keep_for_snapshots`]) deletes an unused file
//! only once a snapshot begun after it became unused is written; one that
//! does not (a replicated member, which applies every committed round again
//! and names no earlier file) deletes it at once.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

mod queue;
mod writer;

pub use queue::{ColdBatch, ColdQueue, MAX_FILE_BYTES};
pub use writer::ColdWriter;

const SUFFIX: &str = ".cold";
const HEADER_BYTES: u64 = 8;

/// Where one image is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    /// Node-wide file id.
    pub file: u64,
    /// Offset of the record header.
    pub offset: u64,
    /// Length of the image.
    pub length: u32,
}

/// The CRC32 a record carries for `image`, to recognise the same bytes.
#[must_use]
pub fn checksum(image: &[u8]) -> u32 {
    crc32fast::hash(image)
}

/// A failed read.
#[derive(Debug)]
pub enum ColdError {
    /// The file could not be opened or read.
    Io(io::Error),
    /// The bytes at the location are not the record written there.
    Corrupt,
}

impl std::fmt::Display for ColdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "cold file read failed: {error}"),
            Self::Corrupt => formatter.write_str("cold record does not match its location"),
        }
    }
}

impl std::error::Error for ColdError {}

impl From<io::Error> for ColdError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Default)]
struct Usage {
    stubs: u64,
    sealed: bool,
    /// The snapshot count when the file became unused, while it waits for
    /// a snapshot that no longer names it.
    unused_since: Option<u64>,
}

#[derive(Default)]
struct Files {
    usage: HashMap<u64, Usage>,
    /// Snapshots begun, when unused files wait for one.
    snapshots: Option<u64>,
}

/// The node's cold files.
pub struct ColdStore {
    directory: PathBuf,
    next: AtomicU64,
    readers: Mutex<HashMap<u64, Arc<File>>>,
    files: Mutex<Files>,
}

/// Critical sections only move values, so a poisoned lock still holds
/// consistent state.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn file_id(name: &str) -> Option<u64> {
    name.strip_suffix(SUFFIX)?.parse().ok()
}

impl ColdStore {
    /// Opens (creating) the cold directory. Files already there are
    /// sealed: no writer appends to them again.
    ///
    /// # Errors
    /// The directory cannot be created or listed.
    pub fn open(directory: &Path) -> io::Result<Arc<Self>> {
        fs::create_dir_all(directory)?;
        let store = Self {
            directory: directory.to_path_buf(),
            next: AtomicU64::new(0),
            readers: Mutex::new(HashMap::new()),
            files: Mutex::new(Files::default()),
        };
        let files = store.files()?;
        let next = files.iter().max().map_or(0, |last| last.saturating_add(1));
        store.next.store(next, Ordering::Relaxed);
        {
            let mut state = lock(&store.files);
            for file in files {
                state.usage.entry(file).or_default().sealed = true;
            }
        }
        Ok(Arc::new(store))
    }

    /// Keeps each unused file until a snapshot begun after it became
    /// unused is written ([`Self::snapshot_written`]), since replay from
    /// the last snapshot reads the files it names.
    pub fn keep_for_snapshots(&self) {
        lock(&self.files).snapshots.get_or_insert(0);
    }

    /// Marks the start of a snapshot's capture; returns its number for
    /// [`Self::snapshot_written`], or `None` when the store does not keep
    /// files for snapshots.
    #[must_use]
    pub fn snapshot_begins(&self) -> Option<u64> {
        let mut state = lock(&self.files);
        let begun = state.snapshots?;
        state.snapshots = Some(begun.saturating_add(1));
        Some(begun)
    }

    /// Deletes the files unused before snapshot `begun` began, now that it
    /// is written and names none of them.
    ///
    /// # Errors
    /// A file cannot be deleted; it is tried again after the next snapshot.
    pub fn snapshot_written(&self, begun: u64) -> io::Result<()> {
        let unused: Vec<u64> = lock(&self.files)
            .usage
            .iter()
            .filter(|(_, usage)| {
                usage.sealed
                    && usage.stubs == 0
                    && usage.unused_since.is_some_and(|since| since <= begun)
            })
            .map(|(file, _)| *file)
            .collect();
        for file in unused {
            self.delete(file)?;
        }
        Ok(())
    }

    /// The files stubs point into now: after a snapshot is loaded, the
    /// files it names.
    #[must_use]
    pub fn held(&self) -> HashSet<u64> {
        lock(&self.files)
            .usage
            .iter()
            .filter(|(_, usage)| usage.stubs > 0)
            .map(|(file, _)| *file)
            .collect()
    }

    fn path(&self, file: u64) -> PathBuf {
        self.directory.join(format!("{file:020}{SUFFIX}"))
    }

    /// The ids of the files present.
    ///
    /// # Errors
    /// The directory cannot be listed.
    pub fn files(&self) -> io::Result<Vec<u64>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let name = entry?.file_name();
            if let Some(file) = name.to_str().and_then(file_id) {
                files.push(file);
            }
        }
        files.sort_unstable();
        Ok(files)
    }

    /// Starts a new file for one writer.
    ///
    /// # Errors
    /// The file cannot be created.
    pub fn writer(self: &Arc<Self>) -> io::Result<ColdWriter> {
        let file = self.next.fetch_add(1, Ordering::Relaxed);
        let handle = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(self.path(file))?;
        lock(&self.files).usage.entry(file).or_default();
        Ok(ColdWriter {
            store: Arc::clone(self),
            handle,
            file,
            length: 0,
            buffer: Vec::new(),
            dirty: false,
            named: false,
        })
    }

    fn reader(&self, file: u64) -> io::Result<Arc<File>> {
        if let Some(reader) = lock(&self.readers).get(&file) {
            return Ok(Arc::clone(reader));
        }
        let reader = Arc::new(File::open(self.path(file))?);
        lock(&self.readers).insert(file, Arc::clone(&reader));
        Ok(reader)
    }

    /// Reads the image at `location`.
    ///
    /// # Errors
    /// The file is missing or unreadable, or the record there does not
    /// match its length and checksum.
    pub fn read(&self, location: Location) -> Result<Vec<u8>, ColdError> {
        let reader = self.reader(location.file)?;
        let mut header = [0_u8; 8];
        reader.read_exact_at(&mut header, location.offset)?;
        let [l0, l1, l2, l3, c0, c1, c2, c3] = header;
        if u32::from_be_bytes([l0, l1, l2, l3]) != location.length {
            return Err(ColdError::Corrupt);
        }
        let length = usize::try_from(location.length).map_err(|_| ColdError::Corrupt)?;
        let mut image = vec![0_u8; length];
        let start = location
            .offset
            .checked_add(HEADER_BYTES)
            .ok_or(ColdError::Corrupt)?;
        reader.read_exact_at(&mut image, start)?;
        if crc32fast::hash(&image) != u32::from_be_bytes([c0, c1, c2, c3]) {
            return Err(ColdError::Corrupt);
        }
        Ok(image)
    }

    /// Counts `count` more stubs in `file`.
    pub fn hold(&self, file: u64, count: u64) {
        let mut state = lock(&self.files);
        let entry = state.usage.entry(file).or_default();
        entry.stubs = entry.stubs.saturating_add(count);
        entry.unused_since = None;
    }

    /// Counts `count` fewer stubs in `file`; a sealed file with none left is
    /// unused.
    ///
    /// # Errors
    /// An unused file cannot be deleted; it is tried again when another
    /// stub of it goes, or by [`Self::remove_unreferenced`].
    pub fn release(&self, file: u64, count: u64) -> io::Result<()> {
        let mut state = lock(&self.files);
        let entry = state.usage.entry(file).or_default();
        entry.stubs = entry.stubs.saturating_sub(count);
        let unused = entry.sealed && entry.stubs == 0;
        self.unused(state, file, unused)
    }

    fn seal(&self, file: u64) -> io::Result<()> {
        let mut state = lock(&self.files);
        let entry = state.usage.entry(file).or_default();
        entry.sealed = true;
        let unused = entry.stubs == 0;
        self.unused(state, file, unused)
    }

    /// Deletes `file` when `unused`, or marks it to wait for a snapshot.
    fn unused(&self, mut state: MutexGuard<'_, Files>, file: u64, unused: bool) -> io::Result<()> {
        if !unused {
            return Ok(());
        }
        let Some(begun) = state.snapshots else {
            drop(state);
            return self.delete(file);
        };
        if let Some(usage) = state.usage.get_mut(&file) {
            usage.unused_since.get_or_insert(begun);
        }
        Ok(())
    }

    fn delete(&self, file: u64) -> io::Result<()> {
        lock(&self.readers).remove(&file);
        lock(&self.files).usage.remove(&file);
        match fs::remove_file(self.path(file)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// After a restart, handles every sealed file not in `referenced`: one
    /// the loaded snapshot names (`named`) waits for the next snapshot when
    /// the store keeps files for snapshots; any other is deleted (records
    /// written after the last snapshot, which replay wrote again elsewhere,
    /// or files whose last stub went).
    ///
    /// # Errors
    /// The directory cannot be listed or a file deleted.
    pub fn remove_unreferenced(
        &self,
        referenced: &HashSet<u64>,
        named: &HashSet<u64>,
    ) -> io::Result<()> {
        for file in self.unreferenced(referenced, named) {
            self.delete(file)?;
        }
        Ok(())
    }

    /// Marks the unreferenced files `named` names to wait for a snapshot,
    /// when the store keeps files for snapshots; returns the others.
    fn unreferenced(&self, referenced: &HashSet<u64>, named: &HashSet<u64>) -> Vec<u64> {
        let mut state = lock(&self.files);
        let snapshots = state.snapshots;
        let mut doomed = Vec::new();
        let unreferenced = state
            .usage
            .iter_mut()
            .filter(|(file, usage)| usage.sealed && usage.stubs == 0 && !referenced.contains(file));
        for (file, usage) in unreferenced {
            match snapshots {
                Some(begun) if named.contains(file) => {
                    usage.unused_since.get_or_insert(begun);
                }
                _ => doomed.push(*file),
            }
        }
        doomed
    }
}
