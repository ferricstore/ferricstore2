//! Delegated real disk reads, armed only at the value-acquisition boundary.

#[cfg(test)]
pub(super) mod fixture_data {
    use redb::{BackendError, StorageBackend, backends::FileBackend};
    use std::{
        fs::OpenOptions,
        io,
        ops::Bound,
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };
    use wf_ports::clock::WorkflowValueReadClock;
    use wf_ports::state::StateTransactionError;
    use wf_types::version::{HlcTimestamp, TimestampMillis};

    use super::super::TestResult;

    #[derive(Debug)]
    struct ReadFaultBackend {
        inner: FileBackend,
        arm: Arc<AtomicBool>,
        hits: Arc<AtomicUsize>,
    }

    impl StorageBackend for ReadFaultBackend {
        fn len(&self) -> io::Result<u64> {
            self.inner.len()
        }
        fn read(&self, offset: u64, output: &mut [u8]) -> io::Result<()> {
            if self.arm.swap(false, Ordering::SeqCst) {
                self.hits.fetch_add(1, Ordering::SeqCst);
                return Err(io::Error::other(
                    "missing private segment /private/secret/history-value-read",
                ));
            }
            self.inner.read(offset, output)
        }
        fn write(&self, offset: u64, input: &[u8]) -> io::Result<()> {
            self.inner.write(offset, input)
        }
        fn set_len(&self, length: u64) -> io::Result<()> {
            self.inner.set_len(length)
        }
        fn sync_data(&self) -> io::Result<()> {
            self.inner.sync_data()
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
        fn lock_shared_range(
            &self,
            start: Bound<u64>,
            end: Bound<u64>,
        ) -> Result<(), BackendError> {
            self.inner.lock_shared_range(start, end)
        }
        fn unlock_range(&self, start: Bound<u64>, end: Bound<u64>) -> Result<(), BackendError> {
            self.inner.unlock_range(start, end)
        }
        fn query_lock_range(
            &self,
            start: Bound<u64>,
            end: Bound<u64>,
        ) -> Result<bool, BackendError> {
            self.inner.query_lock_range(start, end)
        }
    }

    pub(in super::super) struct ReadFaultClock {
        pub(in super::super) arm: Arc<AtomicBool>,
        pub(in super::super) calls: usize,
    }

    impl WorkflowValueReadClock for ReadFaultClock {
        fn read_snapshot(
            &mut self,
        ) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError> {
            self.calls = self
                .calls
                .checked_add(1)
                .ok_or(StateTransactionError::Limit)?;
            self.arm.store(true, Ordering::SeqCst);
            Ok((
                HlcTimestamp::new(101, 0).map_err(|_| StateTransactionError::Corrupt)?,
                TimestampMillis::new(101),
            ))
        }
    }

    pub(in super::super) fn database(
        path: &Path,
        arm: Arc<AtomicBool>,
        hits: Arc<AtomicUsize>,
    ) -> TestResult<redb::Database> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        let backend = ReadFaultBackend {
            inner: FileBackend::new(file)?,
            arm,
            hits,
        };
        Ok(redb::Builder::new()
            .set_cache_size(0)
            .create_with_backend(backend)?)
    }
}
