//! A replicated group's snapshots (ADR-018), written and read as streams so
//! no snapshot is ever in memory whole.
//!
//! A file is the magic bytes, the snapshot's id, its body's length, the
//! body, and a SHA-256 over the magic bytes, the id, the body and the
//! length. The length goes into its place last, so a body streams out
//! before its size is known. A snapshot is written to a temp file, then
//! finished: synced and renamed to a finished (not yet adopted) file. One
//! is adopted when its member holds it: renamed to an adopted file, after
//! which the other adopted files and the finished ones before it are
//! removed (a finished one after it is still being installed or offered).
//! The newest adopted file is what a restart restores. A reader checks the
//! digest as it reads: the read that reaches the body's end fails when the
//! bytes differ from what was written. Opening the directory removes temp
//! and finished files left by a stopped process.
//!
//! A snapshot costs one file sync (when finished) and one directory sync
//! (when adopted): a finished file that a crash loses is one the restart
//! would have removed, and an older adopted file a crash keeps is one the
//! restart passes over for the newest. Closing the store (its member
//! stops) refuses every later creation, finish and adoption, and leaves
//! the files of work in flight where they are, so the directory stays as
//! it was for the store opened on it next.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use wf_ports::consensus::log::{ConsensusLogError, SnapshotSink, SnapshotSource, SnapshotStore};

use super::{SnapshotError, create_durably, number, sync_directory};

const MAGIC: &[u8; 8] = b"WFGSNP01";
const ADOPTED: &str = ".gsnap";
const FINISHED: &str = ".gsnap.done";
const TEMP: &str = ".gsnap.tmp";
/// The magic bytes, the id and the length.
const HEADER_BYTES: u64 = 24;
const DIGEST_BYTES: u64 = 32;
/// Bytes a sink or source buffers between the caller and the file.
const BUFFER_BYTES: usize = 1024 * 1024;

fn failed(error: &impl std::fmt::Display) -> ConsensusLogError {
    ConsensusLogError::new(format!("group snapshot: {error}"))
}

fn closed() -> io::Error {
    io::Error::other("the group's snapshot store is closed")
}

fn corrupt(id: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("group snapshot {id} is corrupt"),
    )
}

/// The directory of a group's snapshots.
#[derive(Clone, Debug)]
pub struct GroupSnapshots {
    directory: PathBuf,
    /// Set once the store is closed; shared with its sinks.
    closed: Arc<AtomicBool>,
}

impl GroupSnapshots {
    /// Uses `directory`, creating it and missing parents, and removes temp
    /// and finished files a stopped process left.
    ///
    /// # Errors
    /// [`SnapshotError::Unavailable`] when the directory cannot be created,
    /// listed or cleaned.
    pub fn open(directory: &Path) -> Result<Self, SnapshotError> {
        create_durably(directory)?;
        let snapshots = Self {
            directory: directory.to_path_buf(),
            closed: Arc::new(AtomicBool::new(false)),
        };
        // A file already gone is fine: a stopped store's last step (a rename
        // under way as it closed) may move it meanwhile.
        for suffix in [TEMP, FINISHED] {
            for id in snapshots.ids(suffix)? {
                snapshots
                    .remove(id, suffix)
                    .map_err(|_| SnapshotError::Unavailable)?;
            }
        }
        Ok(snapshots)
    }

    fn path(&self, id: u64, suffix: &str) -> PathBuf {
        self.directory.join(format!("{id:020}{suffix}"))
    }

    /// The ids of the files with `suffix`, oldest first.
    fn ids(&self, suffix: &str) -> Result<Vec<u64>, SnapshotError> {
        let unavailable = |_| SnapshotError::Unavailable;
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(unavailable)? {
            let name = entry.map_err(unavailable)?.file_name();
            if let Some(id) = name.to_str().and_then(|name| number(name, suffix)) {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    fn remove(&self, id: u64, suffix: &str) -> io::Result<()> {
        match fs::remove_file(self.path(id, suffix)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// A snapshot being written.
struct Sink {
    id: u64,
    out: Option<BufWriter<File>>,
    temp: PathBuf,
    finished: PathBuf,
    closed: Arc<AtomicBool>,
    /// Renamed to its finished file.
    renamed: bool,
    digest: Sha256,
    length: u64,
}

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let out = self
            .out
            .as_mut()
            .ok_or_else(|| io::Error::other("group snapshot already finished"))?;
        let written = out.write(bytes)?;
        let taken = bytes.get(..written).unwrap_or_default();
        self.digest.update(taken);
        self.length = self
            .length
            .checked_add(u64::try_from(written).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("group snapshot too large"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.as_mut().map_or(Ok(()), Write::flush)
    }
}

impl Sink {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn complete(&mut self) -> io::Result<u64> {
        if self.is_closed() {
            return Err(closed());
        }
        let out = self
            .out
            .take()
            .ok_or_else(|| io::Error::other("group snapshot already finished"))?;
        let mut file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
        let mut digest = std::mem::take(&mut self.digest);
        digest.update(self.length.to_be_bytes());
        file.write_all(&digest.finalize())?;
        file.write_all_at(&self.length.to_be_bytes(), 16)?;
        file.sync_all()?;
        drop(file);
        // The sync may take long: the store may have closed meanwhile.
        if self.is_closed() {
            return Err(closed());
        }
        fs::rename(&self.temp, &self.finished)?;
        self.renamed = true;
        Ok(self.length)
    }
}

impl SnapshotSink for Sink {
    fn finish(mut self: Box<Self>) -> Result<u64, ConsensusLogError> {
        let id = self.id;
        self.complete()
            .map_err(|error| failed(&format!("snapshot {id}: {error}")))
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        // Not finished: nothing of it may stay visible, unless the store
        // closed, when its files are left to the store opened next.
        if !self.renamed && !self.is_closed() {
            drop(fs::remove_file(&self.temp));
        }
    }
}

/// A finished snapshot being read, its digest checked at its end.
struct Source {
    id: u64,
    input: BufReader<io::Take<File>>,
    digest: Sha256,
    expected: [u8; 32],
    length: u64,
    left: u64,
}

impl Read for Source {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let read = self.input.read(buffer)?;
        if read == 0 {
            return Err(corrupt(self.id));
        }
        self.digest.update(buffer.get(..read).unwrap_or_default());
        self.left = self
            .left
            .saturating_sub(u64::try_from(read).map_err(io::Error::other)?);
        if self.left == 0 {
            let mut digest = std::mem::take(&mut self.digest);
            digest.update(self.length.to_be_bytes());
            if digest.finalize().as_slice() != self.expected {
                return Err(corrupt(self.id));
            }
        }
        Ok(read)
    }
}

impl SnapshotSource for Source {
    fn length(&self) -> u64 {
        self.length
    }
}

impl GroupSnapshots {
    fn source(&self, id: u64) -> io::Result<Source> {
        let path = [ADOPTED, FINISHED]
            .iter()
            .map(|suffix| self.path(id, suffix))
            .find(|path| path.exists())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such group snapshot"))?;
        let file = File::open(path)?;
        let mut header = [0; 24];
        file.read_exact_at(&mut header, 0)?;
        let (magic, numbers) = header
            .split_at_checked(MAGIC.len())
            .ok_or_else(|| corrupt(id))?;
        let (named, length) = numbers.split_at_checked(8).ok_or_else(|| corrupt(id))?;
        let named = u64::from_be_bytes(named.try_into().map_err(io::Error::other)?);
        let length = u64::from_be_bytes(length.try_into().map_err(io::Error::other)?);
        let size = file.metadata()?.len();
        let whole = HEADER_BYTES
            .checked_add(length)
            .and_then(|bytes| bytes.checked_add(DIGEST_BYTES));
        if magic != MAGIC || named != id || whole != Some(size) {
            return Err(corrupt(id));
        }
        let mut expected = [0; 32];
        file.read_exact_at(&mut expected, HEADER_BYTES.saturating_add(length))?;
        let mut digest = Sha256::new();
        digest.update(MAGIC);
        digest.update(id.to_be_bytes());
        let mut file = file;
        io::Seek::seek(&mut file, io::SeekFrom::Start(HEADER_BYTES))?;
        Ok(Source {
            id,
            input: BufReader::with_capacity(BUFFER_BYTES, file.take(length)),
            digest,
            expected,
            length,
            left: length,
        })
    }

    fn sink(&self, id: u64) -> io::Result<Sink> {
        if self.is_closed() {
            return Err(closed());
        }
        let temp = self.path(id, TEMP);
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        file.write_all(MAGIC)?;
        file.write_all(&id.to_be_bytes())?;
        file.write_all(&0_u64.to_be_bytes())?;
        let mut digest = Sha256::new();
        digest.update(MAGIC);
        digest.update(id.to_be_bytes());
        Ok(Sink {
            id,
            out: Some(BufWriter::with_capacity(BUFFER_BYTES, file)),
            temp,
            finished: self.path(id, FINISHED),
            closed: Arc::clone(&self.closed),
            renamed: false,
            digest,
            length: 0,
        })
    }

    fn adopt_now(&self, id: u64) -> io::Result<()> {
        if self.is_closed() {
            return Err(closed());
        }
        let adopted = self.path(id, ADOPTED);
        let finished = self.path(id, FINISHED);
        if finished.exists() {
            fs::rename(&finished, &adopted)?;
        } else if !adopted.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no finished group snapshot to adopt",
            ));
        }
        sync_directory(&self.directory).map_err(io::Error::other)?;
        let other = |error: SnapshotError| io::Error::other(error.to_string());
        for older in self.ids(ADOPTED).map_err(other)? {
            if older != id {
                self.remove(older, ADOPTED)?;
            }
        }
        for older in self.ids(FINISHED).map_err(other)? {
            if older < id {
                self.remove(older, FINISHED)?;
            }
        }
        Ok(())
    }
}

/// The group's snapshot store (ADR-018): the port over these files.
impl SnapshotStore for GroupSnapshots {
    fn create(&self, id: u64) -> Result<Box<dyn SnapshotSink>, ConsensusLogError> {
        let sink = self.sink(id).map_err(|error| failed(&error))?;
        Ok(Box::new(sink))
    }

    fn adopt(&self, id: u64) -> Result<(), ConsensusLogError> {
        self.adopt_now(id).map_err(|error| failed(&error))
    }

    fn newest(&self) -> Result<Option<u64>, ConsensusLogError> {
        Ok(self
            .ids(ADOPTED)
            .map_err(|error| failed(&error))?
            .last()
            .copied())
    }

    fn open(&self, id: u64) -> Result<Box<dyn SnapshotSource>, ConsensusLogError> {
        let source = self.source(id).map_err(|error| failed(&error))?;
        Ok(Box::new(source))
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}
