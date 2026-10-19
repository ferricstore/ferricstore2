//! Delegated file-sync failures and acknowledged process termination boundaries.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::ops::Bound;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use redb::backends::FileBackend;
use redb::{BackendError, StorageBackend};
use wf_ports::log::PreparedBytes;
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionError, RecordBatchTransactionOutcome, apply_record_batch_transaction,
};

use super::{TestResult, fixtures};

mod expected;
use expected::expected;

#[derive(Debug)]
struct DiskFaultBackend {
    inner: FileBackend,
    fault: Arc<AtomicU8>,
}

impl StorageBackend for DiskFaultBackend {
    fn len(&self) -> io::Result<u64> {
        self.inner.len()
    }
    fn read(&self, offset: u64, output: &mut [u8]) -> io::Result<()> {
        self.inner.read(offset, output)
    }
    fn write(&self, offset: u64, input: &[u8]) -> io::Result<()> {
        self.inner.write(offset, input)
    }
    fn set_len(&self, length: u64) -> io::Result<()> {
        self.inner.set_len(length)
    }
    fn sync_data(&self) -> io::Result<()> {
        let mode = self.fault.swap(0, Ordering::SeqCst);
        if mode == 2 {
            parked("before-sync")?;
        }
        self.inner.sync_data()?;
        if mode == 1 {
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

fn wrapped(path: &Path, fault: Arc<AtomicU8>) -> TestResult<RedbStateStore> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    let backend = DiskFaultBackend {
        inner: FileBackend::new(file)?,
        fault,
    };
    let database = redb::Builder::new()
        .set_cache_size(16 * fixtures::MIB)
        .create_with_backend(backend)?;
    let scope = fixtures::scope()?;
    Ok(RedbStateStore::open_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?)
}

fn seed_frame() -> TestResult<PreparedBytes> {
    fixtures::frame(b"seed", 1_000, vec![(b"i", fixtures::create_op())])
}

fn target(kind: u8) -> TestResult<(PreparedBytes, &'static [u8], u64)> {
    let (request, now, id, operation): (&[u8], _, &[u8], _) = match kind {
        0 => (b"fresh", 2_000, b"j", fixtures::create_op()),
        1 => (b"cancel", 2_000, b"i", fixtures::cancel_op(1)),
        2 => (b"rejected", 2_000, b"i", fixtures::create_op()),
        3 => (b"seed", 1_000, b"i", fixtures::create_op()),
        _ => return Err("unknown fault operation".into()),
    };
    Ok((
        fixtures::frame(request, now, vec![(id, operation)])?,
        request,
        now,
    ))
}

fn perform(
    store: &mut RedbStateStore,
    kind: u8,
    index: u64,
) -> TestResult<RecordBatchTransactionOutcome> {
    let (frame, _, _) = target(kind)?;
    Ok(apply_record_batch_transaction(
        store,
        &fixtures::scope()?,
        frame.as_bytes(),
        fixtures::log(index),
        fixtures::driver_limits(),
    )?)
}

fn initialized(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    drop(fixtures::create(path)?);
    let genesis = fixtures::raw_rows(path)?;
    let mut store = fixtures::open(path)?;
    let frame = seed_frame()?;
    let result = apply_record_batch_transaction(
        &mut store,
        &fixtures::scope()?,
        frame.as_bytes(),
        fixtures::log(0),
        fixtures::driver_limits(),
    )?;
    assert!(matches!(
        result,
        RecordBatchTransactionOutcome::Retained { .. }
    ));
    drop(store);
    assert_eq!(fixtures::raw_rows(path)?, expected(&genesis, None, 0)?);
    Ok(genesis)
}

fn assert_fenced(store: &mut RedbStateStore) -> TestResult {
    let scope = fixtures::scope()?;
    let limits = fixtures::driver_limits();
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            limits.codec.admission(),
            limits.maximum_read_steps,
            limits.maximum_read_bytes,
            limits.maximum_adapter_bytes
        ),
        Err(StateTransactionError::Unavailable)
    ));
    for kind in 0..4 {
        assert!(matches!(perform(store, kind, 2), Err(error)
            if error.downcast_ref::<RecordBatchTransactionError>() ==
                Some(&RecordBatchTransactionError::Transaction(StateTransactionError::Unavailable))));
    }
    assert_eq!(store.group().as_bytes(), b"g");
    assert_eq!(store.recovery().as_bytes(), b"z");
    Ok(())
}

fn reconcile(
    path: &Path,
    genesis: &BTreeMap<Vec<u8>, Vec<u8>>,
    kind: u8,
    changed: bool,
) -> TestResult {
    let index = if changed { 2 } else { 1 };
    let mut store = fixtures::open(path)?;
    let result = perform(&mut store, kind, index)?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("reconciliation did not return retained authority".into());
    };
    assert_eq!(control.position().get(), index);
    assert_eq!(
        outcome.original_position(),
        fixtures::log(u64::from(kind != 3))
    );
    assert_eq!(
        outcome.response(),
        if kind == 2 { &[1, 0, 1][..] } else { &[0][..] }
    );
    drop(outcome);
    drop(store);
    assert_eq!(
        fixtures::raw_rows(path)?,
        expected(genesis, Some(kind), index)?
    );
    Ok(())
}

#[test]
fn immediate_failure_fences_and_reconciles() -> TestResult {
    let dir = fixtures::directory("sync-failure")?;
    for kind in 0..4 {
        let path = dir.join(format!("operation-{kind}"));
        let genesis = initialized(&path)?;
        let manifest = fixtures::raw_manifest(&path)?;
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped(&path, Arc::clone(&fault))?;
        fault.store(1, Ordering::SeqCst);
        assert!(matches!(perform(&mut store, kind, 1), Err(error)
            if error.downcast_ref::<RecordBatchTransactionError>() ==
                Some(&RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain))));
        assert_eq!(fault.load(Ordering::SeqCst), 0, "reached delegated sync");
        assert_fenced(&mut store)?;
        drop(store);
        let actual = fixtures::raw_rows(&path)?;
        let before = expected(&genesis, None, 0)?;
        let after = expected(&genesis, Some(kind), 1)?;
        assert!(
            actual == before || actual == after,
            "mixed cohort after operation {kind}"
        );
        assert_eq!(fixtures::raw_manifest(&path)?, manifest);
        reconcile(&path, &genesis, kind, actual == after)?;
        assert_eq!(fixtures::raw_manifest(&path)?, manifest);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn parked(stage: &str) -> io::Result<()> {
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

fn terminate(child: &mut Child) -> io::Result<ExitStatus> {
    let killed = child.kill();
    let waited = child.wait();
    killed?;
    waited
}

fn signal_reader(
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

fn killed_child(path: &Path, kind: u8, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("faults::apply_kill_ack_boundaries")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("WF_STATE_CHILD_PATH", path)
        .env("WF_STATE_CHILD_KIND", kind.to_string())
        .env("WF_STATE_CHILD_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let Some(stdout) = child.stdout.take() else {
        terminate(&mut child)?;
        return Err("child stdout absent".into());
    };
    let Some(mut stdin) = child.stdin.take() else {
        terminate(&mut child)?;
        return Err("child stdin absent".into());
    };
    let (sender, receiver) = mpsc::sync_channel(2);
    let reader = match signal_reader(stdout, sender, after) {
        Ok(reader) => reader,
        Err(error) => {
            terminate(&mut child)?;
            return Err(error.into());
        }
    };
    let acknowledged = (|| -> TestResult {
        receiver.recv_timeout(Duration::from_secs(20))??;
        stdin.write_all(b"A")?;
        stdin.flush()?;
        receiver.recv_timeout(Duration::from_secs(20))??;
        Ok(())
    })();
    let terminated = terminate(&mut child);
    drop(stdin);
    let joined = reader.join();
    acknowledged?;
    assert!(!terminated?.success());
    joined.map_err(|_| "child signal reader panicked")??;
    Ok(())
}

fn child(path: &Path, kind: u8, after: bool) -> TestResult {
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(path, Arc::clone(&fault))?;
    if !after {
        fault.store(2, Ordering::SeqCst);
    }
    let result = perform(&mut store, kind, 1)?;
    assert!(matches!(
        result,
        RecordBatchTransactionOutcome::Retained { .. }
    ));
    if after {
        parked("after-return")?;
    }
    Err("child returned instead of parking at the requested boundary".into())
}

#[test]
fn apply_kill_ack_boundaries() -> TestResult {
    if let Some(path) = std::env::var_os("WF_STATE_CHILD_PATH") {
        return child(
            Path::new(&path),
            std::env::var("WF_STATE_CHILD_KIND")?.parse()?,
            std::env::var("WF_STATE_CHILD_AFTER")? == "1",
        );
    }
    let dir = fixtures::directory("kill")?;
    for kind in 0..4 {
        for after in [false, true] {
            let path = dir.join(format!("operation-{kind}-{after}"));
            let genesis = initialized(&path)?;
            let manifest = fixtures::raw_manifest(&path)?;
            killed_child(&path, kind, after)?;
            let actual = fixtures::raw_rows(&path)?;
            let old = expected(&genesis, None, 0)?;
            let new = expected(&genesis, Some(kind), 1)?;
            if after {
                assert_eq!(actual, new);
            } else {
                assert!(actual == old || actual == new, "mixed cohort for {kind}");
            }
            assert_eq!(fixtures::raw_manifest(&path)?, manifest);
            reconcile(&path, &genesis, kind, actual == new)?;
            assert_eq!(fixtures::raw_manifest(&path)?, manifest);
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
