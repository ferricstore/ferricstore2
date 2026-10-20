//! Shared delegated disk failures and acknowledged process boundaries.

use super::TestResult;
use redb::backends::FileBackend;
use redb::{BackendError, StorageBackend};
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::ops::Bound;
use std::path::Path;
use std::process::{Child, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

#[derive(Debug)]
struct DiskFaultBackend {
    inner: FileBackend,
    fault: Arc<AtomicU8>,
    read_probe: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
    write_probe: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
}

impl StorageBackend for DiskFaultBackend {
    fn len(&self) -> io::Result<u64> {
        self.inner.len()
    }
    fn read(&self, offset: u64, output: &mut [u8]) -> io::Result<()> {
        if self
            .fault
            .compare_exchange(3, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(io::Error::other("injected control getter read failure"));
        }
        check_probe(self.read_probe.as_ref())?;
        self.inner.read(offset, output)
    }
    fn write(&self, offset: u64, input: &[u8]) -> io::Result<()> {
        check_probe(self.write_probe.as_ref())?;
        self.inner.write(offset, input)
    }
    fn set_len(&self, length: u64) -> io::Result<()> {
        let mode = self.fault.load(Ordering::SeqCst);
        let before = if matches!(mode, 4 | 6) {
            Some(self.inner.len()?)
        } else {
            None
        };
        self.inner.set_len(length)?;
        if before.is_some_and(|before| length > before) {
            let _ = self
                .fault
                .compare_exchange(mode, mode + 1, Ordering::SeqCst, Ordering::SeqCst);
        }
        Ok(())
    }
    fn sync_data(&self) -> io::Result<()> {
        let mode = self.fault.swap(0, Ordering::SeqCst);
        if matches!(mode, 2 | 6) {
            parked("before-sync")?;
        }
        self.inner.sync_data()?;
        // The pinned engine syncs each successful growth before publication.
        if matches!(mode, 5 | 7) {
            self.fault.store(mode - 1, Ordering::SeqCst);
        }
        if matches!(mode, 1 | 4) {
            return Err(io::Error::other(
                "injected failure AFTER delegated file sync",
            ));
        }
        Ok(())
    }
    fn close(&self) -> io::Result<()> {
        self.inner.close()
    }
    fn try_lock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<bool, BackendError> {
        self.inner.try_lock_range(start, end)
    }
    fn try_lock_shared_range(
        &self,
        start: Bound<u64>,
        end: Bound<u64>,
    ) -> Result<bool, BackendError> {
        self.inner.try_lock_shared_range(start, end)
    }
    fn lock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<(), BackendError> {
        self.inner.lock_range(start, end)
    }
    fn lock_shared_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<(), BackendError> {
        self.inner.lock_shared_range(start, end)
    }
    fn unlock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<(), BackendError> {
        self.inner.unlock_range(start, end)
    }
    fn query_lock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<bool, BackendError> {
        self.inner.query_lock_range(start, end)
    }
}

pub(crate) fn fault_database(
    path: &Path,
    fault: Arc<AtomicU8>,
    cache_size: usize,
) -> TestResult<redb::Database> {
    fault_database_counted(path, fault, cache_size, None, None)
}

fn check_probe(probe: Option<&(Arc<AtomicUsize>, Arc<AtomicUsize>)>) -> io::Result<()> {
    let Some((attempts, remaining)) = probe else {
        return Ok(());
    };
    attempts.fetch_add(1, Ordering::SeqCst);
    let previous = remaining.try_update(Ordering::SeqCst, Ordering::SeqCst, |left| match left {
        usize::MAX => None,
        0 => Some(usize::MAX),
        left => Some(left - 1),
    });
    if previous == Ok(0) {
        Err(io::Error::other("injected counted disk attempt failure"))
    } else {
        Ok(())
    }
}

/// Counts actual read/write attempts; MAX disables the one-shot failure.
pub(crate) fn fault_database_counted(
    path: &Path,
    fault: Arc<AtomicU8>,
    cache_size: usize,
    read_probe: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
    write_probe: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
) -> TestResult<redb::Database> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let backend = DiskFaultBackend {
        inner: FileBackend::new(file)?,
        fault,
        read_probe,
        write_probe,
    };
    let database = redb::Builder::new()
        .set_cache_size(cache_size)
        .create_with_backend(backend)?;
    Ok(database)
}

pub(crate) fn parked(stage: &str) -> io::Result<()> {
    println!("WFSTATE READY {stage}");
    io::stdout().flush()?;
    let mut acknowledgement = [0; 1];
    io::stdin().read_exact(&mut acknowledgement)?;
    if acknowledgement != *b"A" {
        return Err(io::Error::other("invalid parent acknowledgement"));
    }
    println!("WFSTATE WAIT {stage}");
    io::stdout().flush()?;
    io::stdin().read_exact(&mut acknowledgement)?;
    Err(io::Error::other("parent did not terminate parked process"))
}

fn read_signal(reader: &mut impl Read, token: &[u8]) -> io::Result<()> {
    for _ in 0..32 {
        let mut line = [0; 256];
        let mut length = 0;
        loop {
            if length == line.len() {
                return Err(io::Error::other("oversized child line"));
            }
            reader.read_exact(&mut line[length..=length])?;
            length += 1;
            if line[length - 1] == b'\n' {
                break;
            }
        }
        if line[..length]
            .windows(token.len())
            .any(|window| window == token)
        {
            return Ok(());
        }
    }
    Err(io::Error::other("missing acknowledged child boundary"))
}

pub(crate) fn terminate(child: &mut Child) -> io::Result<ExitStatus> {
    let killed = child.kill();
    let waited = child.wait();
    killed?;
    waited
}

pub(crate) fn signal_reader(
    mut stdout: std::process::ChildStdout,
    sender: mpsc::SyncSender<io::Result<()>>,
    after: bool,
) -> io::Result<thread::JoinHandle<io::Result<()>>> {
    thread::Builder::new()
        .name("state-crash-ack".into())
        .spawn(move || {
            let ready = if after {
                b"WFSTATE READY after-return\n".as_slice()
            } else {
                b"WFSTATE READY before-sync\n"
            };
            let wait = if after {
                b"WFSTATE WAIT after-return\n".as_slice()
            } else {
                b"WFSTATE WAIT before-sync\n"
            };
            let result = read_signal(&mut stdout, ready);
            let ok = result.is_ok();
            sender.send(result).map_err(io::Error::other)?;
            if ok {
                sender
                    .send(read_signal(&mut stdout, wait))
                    .map_err(io::Error::other)?;
            }
            Ok(())
        })
}
