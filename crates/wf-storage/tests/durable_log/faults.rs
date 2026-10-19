//! Real delegated file sync errors and acknowledged process-kill boundaries.

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
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogStore, RaftVote};
use wf_storage::log::RedbLogStore;
use wf_types::identity::{GroupId, RecoveryId};

use super::{
    MIB, TestResult, create, directory, entries, entry_wire, expected_cohort, id, open, raw_cohort,
    state_wire,
};

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
            return Err(io::Error::other("injected error AFTER delegated file sync"));
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

fn wrapped(path: &Path, fault: Arc<AtomicU8>, initialize: bool) -> TestResult<RedbLogStore> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(initialize)
        .truncate(false)
        .open(path)?;
    let backend = DiskFaultBackend {
        inner: FileBackend::new(file)?,
        fault,
    };
    let database = redb::Builder::new()
        .set_cache_size(16 * MIB)
        .create_with_backend(backend)?;
    let group = GroupId::new(b"g".to_vec())?;
    let recovery = RecoveryId::new(b"z".to_vec())?;
    Ok(if initialize {
        RedbLogStore::create_database(database, group, recovery, RaftLogLimits::default())?
    } else {
        RedbLogStore::open_database(database, group, recovery, RaftLogLimits::default())?
    })
}

fn seed(store: &mut RedbLogStore) -> TestResult {
    store.append(entries(&[
        (id(1, 7, 0), b"zero"),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ]))?;
    store.save_vote(RaftVote::new(1, 7, false))?;
    store.save_committed(id(1, 7, 0))?;
    Ok(())
}

fn perform(store: &mut RedbLogStore, kind: u8) -> Result<(), RaftLogError> {
    match kind {
        0 => store.append(entries(&[(id(2, 0, 3), b"three"), (id(2, 0, 4), b"four")])),
        1 => store.save_vote(RaftVote::new(1, 7, true)),
        2 => store.save_committed(id(1, 7, 1)),
        3 => store.truncate_from(1),
        _ => Err(RaftLogError::InvalidRange),
    }
}

fn expected(kind: u8, changed: bool) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut specs = vec![
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ];
    let mut vote = Some((1, 7, false));
    let mut committed = Some(id(1, 7, 0));
    if changed {
        match kind {
            0 => specs.extend([(id(2, 0, 3), b"three".as_slice()), (id(2, 0, 4), b"four")]),
            1 => vote = Some((1, 7, true)),
            2 => committed = Some(id(1, 7, 1)),
            3 => specs.truncate(1),
            _ => return Err("unknown expected fault operation".into()),
        }
    }
    expected_cohort(b"g", b"z", &specs, vote, committed)
}

fn assert_fenced(store: &mut RedbLogStore) {
    assert!(matches!(store.state(), Err(RaftLogError::Fenced)));
    assert!(matches!(store.read_entry(0, 0), Err(RaftLogError::Fenced)));
    assert!(matches!(
        store.read_page(Bound::Unbounded, Bound::Unbounded),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(
        store.save_vote(RaftVote::new(9, 9, true)),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(
        store.save_committed(id(1, 7, 0)),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(
        store.append(entries(&[(id(2, 0, 3), b"later")])),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(store.truncate_from(1), Err(RaftLogError::Fenced)));
    assert_eq!(store.group().as_bytes(), b"g");
    assert_eq!(store.recovery().as_bytes(), b"z");
}

fn expected_resumed(kind: u8, changed: bool, next: u64) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut rows = expected(kind, changed)?;
    let vote = (1, 7, changed && kind == 1);
    let committed = if changed && kind == 2 {
        id(1, 7, 1)
    } else {
        id(1, 7, 0)
    };
    let identity = id(3, 0, next);
    let state = rows
        .iter_mut()
        .find(|(key, _)| key == b"Mstate")
        .ok_or("expected state absent")?;
    state.1 = state_wire(b"g", b"z", Some(vote), Some(committed), Some(identity))?;
    let mut key = vec![b'L'];
    key.extend_from_slice(&next.to_be_bytes());
    rows.push((key, entry_wire(b"g", b"z", identity, b"reconciled")?));
    rows.sort();
    Ok(rows)
}

#[test]
fn log_sync_failure_fences() -> TestResult {
    let dir = directory("sync-failure")?;
    for kind in 0..4 {
        let path = dir.join(format!("operation-{kind}"));
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped(&path, Arc::clone(&fault), true)?;
        seed(&mut store)?;
        fault.store(1, Ordering::SeqCst);
        assert!(matches!(
            perform(&mut store, kind),
            Err(RaftLogError::Uncertain)
        ));
        assert_eq!(fault.load(Ordering::SeqCst), 0, "fault reached actual sync");
        assert_fenced(&mut store);
        drop(store);
        let actual = raw_cohort(&path)?;
        assert!(
            actual == expected(kind, false)? || actual == expected(kind, true)?,
            "mixed cohort after operation {kind}"
        );
        let mut reopened = open(&path, RaftLogLimits::default())?;
        let changed = actual == expected(kind, true)?;
        let next = if changed && kind == 0 {
            5
        } else if changed && kind == 3 {
            1
        } else {
            3
        };
        reopened.append(entries(&[(id(3, 0, next), b"reconciled")]))?;
        assert_eq!(reopened.state()?.last_log(), Some(id(3, 0, next)));
        drop(reopened);
        assert_eq!(raw_cohort(&path)?, expected_resumed(kind, changed, next)?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn parked(stage: &str) -> io::Result<()> {
    println!("WFLOG READY {stage}");
    io::stdout().flush()?;
    let mut acknowledgement = [0; 1];
    io::stdin().read_exact(&mut acknowledgement)?;
    if acknowledgement != *b"A" {
        return Err(io::Error::other("invalid parent acknowledgement"));
    }
    println!("WFLOG WAIT {stage}");
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
    Err(io::Error::other("missing child boundary acknowledgement"))
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
        .name("log-crash-ack".into())
        .spawn(move || {
            let ready_token = if after {
                b"WFLOG READY after-return\n".as_slice()
            } else {
                b"WFLOG READY before-sync\n"
            };
            let wait_token = if after {
                b"WFLOG WAIT after-return\n".as_slice()
            } else {
                b"WFLOG WAIT before-sync\n"
            };
            let ready = read_signal(&mut stdout, ready_token);
            let successful = ready.is_ok();
            sender.send(ready).map_err(io::Error::other)?;
            if successful {
                sender
                    .send(read_signal(&mut stdout, wait_token))
                    .map_err(io::Error::other)?;
            }
            Ok(())
        })
}

fn killed_child(path: &Path, kind: u8, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("faults::log_kill_ack_boundaries")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("WF_LOG_CHILD_PATH", path)
        .env("WF_LOG_CHILD_KIND", kind.to_string())
        .env("WF_LOG_CHILD_AFTER", if after { "1" } else { "0" })
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
    let reader = signal_reader(stdout, sender, after);
    let reader = match reader {
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
    joined.map_err(|_| "child acknowledgement reader panicked")??;
    Ok(())
}

fn child(path: &Path, kind: u8, after: bool) -> TestResult {
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(path, Arc::clone(&fault), false)?;
    assert_eq!(store.state()?.committed(), Some(id(1, 7, 0)));
    if !after {
        fault.store(2, Ordering::SeqCst);
    }
    perform(&mut store, kind)?;
    if after {
        parked("after-return")?;
    }
    Err("child returned instead of parking inside requested boundary".into())
}

#[test]
fn log_kill_ack_boundaries() -> TestResult {
    if let Some(path) = std::env::var_os("WF_LOG_CHILD_PATH") {
        return child(
            Path::new(&path),
            std::env::var("WF_LOG_CHILD_KIND")?.parse()?,
            std::env::var("WF_LOG_CHILD_AFTER")? == "1",
        );
    }
    let dir = directory("kill")?;
    for kind in 0..4 {
        for after in [false, true] {
            let path = dir.join(format!("operation-{kind}-{after}"));
            let mut store = create(&path, RaftLogLimits::default())?;
            seed(&mut store)?;
            drop(store);
            assert_eq!(raw_cohort(&path)?, expected(kind, false)?);
            killed_child(&path, kind, after)?;
            let actual = raw_cohort(&path)?;
            if after {
                assert_eq!(actual, expected(kind, true)?);
            } else {
                assert!(
                    actual == expected(kind, false)? || actual == expected(kind, true)?,
                    "mixed cohort for operation {kind}"
                );
            }
            let mut recovered = open(&path, RaftLogLimits::default())?;
            assert!(recovered.state()?.committed().is_some());
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
