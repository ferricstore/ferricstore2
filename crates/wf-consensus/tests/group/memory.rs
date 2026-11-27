//! A snapshot store in memory for the gate's test machine: the port's
//! streams over byte vectors, finished and adopted as the file store does.

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};

use wf_ports::consensus::log::{ConsensusLogError, SnapshotSink, SnapshotSource, SnapshotStore};

/// Snapshots by id: their bytes and whether adopted.
type Files = Arc<Mutex<BTreeMap<u64, (Arc<Vec<u8>>, bool)>>>;

#[derive(Clone, Default)]
pub(super) struct MemoryStore {
    files: Files,
}

struct Sink {
    id: u64,
    bytes: Vec<u8>,
    files: Files,
}

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SnapshotSink for Sink {
    fn finish(self: Box<Self>) -> Result<u64, ConsensusLogError> {
        let length = u64::try_from(self.bytes.len())
            .map_err(|error| ConsensusLogError::new(error.to_string()))?;
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(self.id, (Arc::new(self.bytes), false));
        Ok(length)
    }
}

struct Source {
    bytes: Cursor<Vec<u8>>,
    length: u64,
}

impl Read for Source {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.bytes.read(buffer)
    }
}

impl SnapshotSource for Source {
    fn length(&self) -> u64 {
        self.length
    }
}

fn missing() -> ConsensusLogError {
    ConsensusLogError::new("no such test snapshot".to_owned())
}

impl SnapshotStore for MemoryStore {
    fn create(&self, id: u64) -> Result<Box<dyn SnapshotSink>, ConsensusLogError> {
        Ok(Box::new(Sink {
            id,
            bytes: Vec::new(),
            files: Arc::clone(&self.files),
        }))
    }

    fn adopt(&self, id: u64) -> Result<(), ConsensusLogError> {
        let mut files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        files.get_mut(&id).ok_or_else(missing)?.1 = true;
        files.retain(|other, (_, adopted)| *other == id || (!*adopted && *other > id));
        Ok(())
    }

    fn newest(&self) -> Result<Option<u64>, ConsensusLogError> {
        let files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(files
            .iter()
            .filter(|(_, (_, adopted))| *adopted)
            .map(|(id, _)| *id)
            .next_back())
    }

    fn open(&self, id: u64) -> Result<Box<dyn SnapshotSource>, ConsensusLogError> {
        let files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = files.get(&id).ok_or_else(missing)?.0.as_ref().clone();
        let length = u64::try_from(bytes.len())
            .map_err(|error| ConsensusLogError::new(error.to_string()))?;
        Ok(Box::new(Source {
            bytes: Cursor::new(bytes),
            length,
        }))
    }

    fn close(&self) {}
}
