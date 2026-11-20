//! Where the group log keeps its numbered segments.
//!
//! [`DirectoryStorage`] keeps each segment as `{number:020}.wfgl` in one
//! directory. Creating and removing a segment syncs the directory so the
//! change survives a crash. Tests wrap it to inject failures.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

const SUFFIX: &str = ".wfgl";
const DIGITS: usize = 20;

/// Storage for the group log's segments. Every method is called from one
/// thread at a time: recovery first, then the writer thread.
pub trait SegmentStorage: Send + Sync + 'static {
    /// Numbers of the segments present, in any order.
    ///
    /// # Errors
    /// Returns the storage's I/O error.
    fn segments(&self) -> io::Result<Vec<u64>>;

    /// The full contents of a segment.
    ///
    /// # Errors
    /// Returns the storage's I/O error.
    fn read(&self, segment: u64) -> io::Result<Vec<u8>>;

    /// Creates an empty segment, durably, and opens it for appending.
    ///
    /// # Errors
    /// Refuses an existing segment and returns the storage's I/O error.
    fn create(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>>;

    /// Opens an existing segment for appending.
    ///
    /// # Errors
    /// Returns the storage's I/O error.
    fn reopen(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>>;

    /// Cuts a segment to `length` bytes, durably.
    ///
    /// # Errors
    /// Returns the storage's I/O error.
    fn truncate(&self, segment: u64, length: u64) -> io::Result<()>;

    /// Removes a segment, durably.
    ///
    /// # Errors
    /// Returns the storage's I/O error.
    fn remove(&self, segment: u64) -> io::Result<()>;
}

/// The open end of the current segment.
pub trait SegmentWriter: Send {
    /// Appends all of `bytes`.
    ///
    /// # Errors
    /// Returns the storage's I/O error; some of `bytes` may have been written.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Makes every appended byte durable.
    ///
    /// # Errors
    /// Returns the storage's I/O error.
    fn sync(&mut self) -> io::Result<()>;
}

/// Segments as files in one directory.
#[derive(Clone, Debug)]
pub struct DirectoryStorage {
    directory: PathBuf,
}

impl DirectoryStorage {
    /// Uses `directory`, creating it (and syncing its parent) when missing.
    ///
    /// # Errors
    /// Returns the I/O error of creating or syncing the directory.
    pub fn open(directory: &Path) -> io::Result<Self> {
        match fs::create_dir(directory) {
            Ok(()) => sync_directory(parent(directory))?,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        Ok(Self {
            directory: directory.to_path_buf(),
        })
    }

    /// The file holding `segment`.
    #[must_use]
    pub fn segment_path(&self, segment: u64) -> PathBuf {
        self.directory.join(format!("{segment:020}{SUFFIX}"))
    }
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn sync_directory(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

/// The segment number of a file name, if it is a segment file.
fn segment_number(name: &str) -> Option<u64> {
    let digits = name.strip_suffix(SUFFIX)?;
    let all_digits = digits.len() == DIGITS && digits.bytes().all(|byte| byte.is_ascii_digit());
    all_digits.then(|| digits.parse().ok()).flatten()
}

struct FileWriter {
    file: File,
}

impl SegmentWriter for FileWriter {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
    }

    fn sync(&mut self) -> io::Result<()> {
        self.file.sync_data()
    }
}

impl SegmentStorage for DirectoryStorage {
    fn segments(&self) -> io::Result<Vec<u64>> {
        let mut segments = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            if let Some(segment) = entry?.file_name().to_str().and_then(segment_number) {
                segments.push(segment);
            }
        }
        Ok(segments)
    }

    fn read(&self, segment: u64) -> io::Result<Vec<u8>> {
        fs::read(self.segment_path(segment))
    }

    fn create(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        let file = OpenOptions::new()
            .append(true)
            .create_new(true)
            .open(self.segment_path(segment))?;
        sync_directory(&self.directory)?;
        Ok(Box::new(FileWriter { file }))
    }

    fn reopen(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        let file = OpenOptions::new()
            .append(true)
            .open(self.segment_path(segment))?;
        Ok(Box::new(FileWriter { file }))
    }

    fn truncate(&self, segment: u64, length: u64) -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .open(self.segment_path(segment))?;
        file.set_len(length)?;
        file.sync_data()
    }

    fn remove(&self, segment: u64) -> io::Result<()> {
        fs::remove_file(self.segment_path(segment))?;
        sync_directory(&self.directory)
    }
}
