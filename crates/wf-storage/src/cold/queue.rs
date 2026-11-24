//! The cold writer thread (ADR-016): shards hand it each round's images;
//! it writes everything queued, syncs each file once, then answers each
//! batch with the images' locations, so no shard thread waits on a file.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use super::{ColdStore, ColdWriter, Location};

/// Images to write for one shard round, with the hour of removal each
/// Flow's file is chosen by, and where the locations go once durable.
pub struct ColdBatch {
    /// Removal hour and image, in order.
    pub images: Vec<(u64, Vec<u8>)>,
    /// Called once with the images' locations, in order, after they are
    /// durable; or with the error that stopped them.
    pub done: Box<dyn FnOnce(io::Result<Vec<Location>>) + Send>,
}

/// Most bytes one cold file takes before the next starts.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// The cold writer thread's queue.
pub struct ColdQueue {
    sender: Option<std::sync::mpsc::Sender<ColdBatch>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ColdQueue {
    /// Starts the writer thread for `store`.
    ///
    /// # Errors
    /// The thread did not start.
    pub fn spawn(store: &Arc<ColdStore>) -> io::Result<Self> {
        Self::spawn_sized(store, MAX_FILE_BYTES)
    }

    /// [`Self::spawn`] with files that take at most about `max_file_bytes`.
    ///
    /// # Errors
    /// The thread did not start.
    pub fn spawn_sized(store: &Arc<ColdStore>, max_file_bytes: u64) -> io::Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel::<ColdBatch>();
        let store = Arc::clone(store);
        let thread = std::thread::Builder::new()
            .name("wf-cold".into())
            .spawn(move || write_loop(&store, &receiver, max_file_bytes))?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    /// Queues one batch; its `done` gets an error when the thread is gone.
    pub fn send(&self, batch: ColdBatch) {
        let refused = match &self.sender {
            Some(sender) => sender.send(batch).err().map(|returned| returned.0),
            None => Some(batch),
        };
        if let Some(batch) = refused {
            (batch.done)(Err(io::Error::other("the cold writer stopped")));
        }
    }
}

impl Drop for ColdQueue {
    fn drop(&mut self) {
        // Closing the channel lets the thread finish what is queued.
        self.sender = None;
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

fn wall_hour() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX) / 3_600_000
        })
}

/// One round of the writer: every queued batch, one sync per file.
fn write_round(
    store: &Arc<ColdStore>,
    (writers, max_file_bytes): (&mut BTreeMap<u64, ColdWriter>, u64),
    batches: Vec<ColdBatch>,
) {
    let mut placed: Vec<(ColdBatch, Vec<Location>, Option<io::Error>)> = Vec::new();
    for mut batch in batches {
        let images = std::mem::take(&mut batch.images);
        let mut locations = Vec::with_capacity(images.len());
        let mut failure = None;
        for (hour, image) in &images {
            match append_to(store, (writers, max_file_bytes), *hour, image) {
                Ok(location) => {
                    // Counted at once: a file sealed full later in this
                    // round still holds this image.
                    store.hold(location.file, 1);
                    locations.push(location);
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        placed.push((batch, locations, failure));
    }
    let synced = writers
        .values_mut()
        .try_for_each(ColdWriter::sync)
        .map_err(|error| error.kind());
    for (batch, locations, failure) in placed {
        let error = match (&synced, failure) {
            (Ok(()), None) => {
                (batch.done)(Ok(locations));
                continue;
            }
            (Ok(()), Some(error)) => error,
            (Err(kind), _) => io::Error::new(*kind, "cold sync failed"),
        };
        // Not handed out: nothing points at what was appended.
        for location in &locations {
            drop(store.release(location.file, 1));
        }
        (batch.done)(Err(error));
    }
    // Flows removed in past hours no longer join their files.
    let current = wall_hour();
    let past: Vec<u64> = writers.range(..current).map(|(hour, _)| *hour).collect();
    for hour in past {
        if let Some(writer) = writers.remove(&hour) {
            drop(writer.seal());
        }
    }
}

fn append_to(
    store: &Arc<ColdStore>,
    (writers, max_file_bytes): (&mut BTreeMap<u64, ColdWriter>, u64),
    hour: u64,
    image: &[u8],
) -> io::Result<Location> {
    if writers
        .get(&hour)
        .is_some_and(|writer| writer.length() >= max_file_bytes)
        && let Some(full) = writers.remove(&hour)
    {
        full.seal()?;
    }
    let writer = match writers.entry(hour) {
        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => entry.insert(store.writer()?),
    };
    writer.append(image)
}

fn write_loop(
    store: &Arc<ColdStore>,
    receiver: &std::sync::mpsc::Receiver<ColdBatch>,
    max_file_bytes: u64,
) {
    let mut writers: BTreeMap<u64, ColdWriter> = BTreeMap::new();
    while let Ok(first) = receiver.recv() {
        let mut batches = vec![first];
        while let Ok(more) = receiver.try_recv() {
            batches.push(more);
        }
        write_round(store, (&mut writers, max_file_bytes), batches);
    }
    for writer in std::mem::take(&mut writers).into_values() {
        drop(writer.seal());
    }
}
