//! Cold files for finished Flows (ADR-016).
//!
//! A shard writes each finished Flow's image to a cold file and keeps only
//! where it is. Files are append-only and written by one [`ColdWriter`]
//! each; after a restart they are only read, never appended to. A record is
//! a `u32` length, the CRC32 of the image and the image, big-endian. File
//! ids are node-wide, so a location keeps working when the shard count
//! changes. Each file counts the stubs that point into it; once its writer
//! is sealed and the count reaches zero, the file is deleted.
//!
//! Durability: [`ColdWriter::sync`] makes appended records durable. A
//! snapshot syncs its shards' writers first, so a stub in a snapshot names
//! durable bytes; anything written later is written again by replay.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

mod queue;

pub use queue::{ColdBatch, ColdQueue};

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
}

/// The node's cold files.
pub struct ColdStore {
    directory: PathBuf,
    next: AtomicU64,
    readers: Mutex<HashMap<u64, Arc<File>>>,
    usage: Mutex<HashMap<u64, Usage>>,
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
            usage: Mutex::new(HashMap::new()),
        };
        let files = store.files()?;
        let next = files.iter().max().map_or(0, |last| last.saturating_add(1));
        store.next.store(next, Ordering::Relaxed);
        {
            let mut usage = lock(&store.usage);
            for file in files {
                usage.entry(file).or_default().sealed = true;
            }
        }
        Ok(Arc::new(store))
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
        lock(&self.usage).entry(file).or_default();
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
        let mut usage = lock(&self.usage);
        let entry = usage.entry(file).or_default();
        entry.stubs = entry.stubs.saturating_add(count);
    }

    /// Counts `count` fewer stubs in `file`; a sealed file with none left is
    /// deleted.
    ///
    /// # Errors
    /// The file cannot be deleted; it is tried again when another stub of
    /// it goes, or by [`Self::remove_unreferenced`].
    pub fn release(&self, file: u64, count: u64) -> io::Result<()> {
        let unused = {
            let mut usage = lock(&self.usage);
            let entry = usage.entry(file).or_default();
            entry.stubs = entry.stubs.saturating_sub(count);
            entry.sealed && entry.stubs == 0
        };
        if unused { self.delete(file) } else { Ok(()) }
    }

    fn seal(&self, file: u64) -> io::Result<()> {
        let unused = {
            let mut usage = lock(&self.usage);
            let entry = usage.entry(file).or_default();
            entry.sealed = true;
            entry.stubs == 0
        };
        if unused { self.delete(file) } else { Ok(()) }
    }

    fn delete(&self, file: u64) -> io::Result<()> {
        lock(&self.readers).remove(&file);
        lock(&self.usage).remove(&file);
        match fs::remove_file(self.path(file)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// Deletes every sealed file not in `referenced`: after a restart, the
    /// files no stub names (records written after the last snapshot, which
    /// replay wrote again elsewhere, or files whose last stub went).
    ///
    /// # Errors
    /// The directory cannot be listed or a file deleted.
    pub fn remove_unreferenced(&self, referenced: &HashSet<u64>) -> io::Result<()> {
        let sealed: Vec<u64> = lock(&self.usage)
            .iter()
            .filter(|(file, usage)| usage.sealed && !referenced.contains(file))
            .map(|(file, _)| *file)
            .collect();
        for file in sealed {
            self.delete(file)?;
        }
        Ok(())
    }
}

/// Appends images to one cold file.
pub struct ColdWriter {
    store: Arc<ColdStore>,
    handle: File,
    file: u64,
    length: u64,
    /// Appended images not yet written to the file.
    buffer: Vec<u8>,
    dirty: bool,
    /// Whether the directory entry of the file is durable.
    named: bool,
}

impl ColdWriter {
    /// The file this writer appends to.
    #[must_use]
    pub const fn file(&self) -> u64 {
        self.file
    }

    /// Bytes appended so far, written or not.
    #[must_use]
    pub fn length(&self) -> u64 {
        self.length
            .saturating_add(u64::try_from(self.buffer.len()).unwrap_or(u64::MAX))
    }

    /// Appends one image to the writer's buffer; readable after the next
    /// [`Self::flush`], durable after the next [`Self::sync`].
    ///
    /// # Errors
    /// The image is larger than a record can hold.
    pub fn append(&mut self, image: &[u8]) -> io::Result<Location> {
        let length = u32::try_from(image.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "cold image too large"))?;
        let location = Location {
            file: self.file,
            offset: self.length(),
            length,
        };
        self.buffer.extend_from_slice(&length.to_be_bytes());
        self.buffer
            .extend_from_slice(&crc32fast::hash(image).to_be_bytes());
        self.buffer.extend_from_slice(image);
        Ok(location)
    }

    /// Writes the buffered images to the file with one call.
    ///
    /// # Errors
    /// The write failed; the buffered images are kept for the next try.
    pub fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        self.handle.write_all(&self.buffer)?;
        self.length = self.length();
        self.buffer.clear();
        self.dirty = true;
        Ok(())
    }

    /// Makes every appended image durable.
    ///
    /// # Errors
    /// The sync failed; the images may or may not survive a crash.
    pub fn sync(&mut self) -> io::Result<()> {
        self.flush()?;
        if self.dirty {
            self.handle.sync_data()?;
            self.dirty = false;
        }
        if !self.named {
            File::open(&self.store.directory)?.sync_all()?;
            self.named = true;
        }
        Ok(())
    }

    /// Syncs and closes the file for appends; it is deleted once no stub
    /// points into it.
    ///
    /// # Errors
    /// The sync or a deletion failed.
    pub fn seal(mut self) -> io::Result<()> {
        self.sync()?;
        self.store.seal(self.file)
    }
}
