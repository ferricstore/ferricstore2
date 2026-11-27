//! A replicated group's snapshot files (ADR-018): written as a stream,
//! visible only once finished, the newest once adopted, and read back as a
//! stream whose end fails when the bytes changed.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wf_ports::consensus::log::SnapshotStore;
use wf_storage::snapshot::files::group::GroupSnapshots;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn directory(tag: &str) -> TestResult<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "wf-group-snapshots-{}-{nanos}-{}-{tag}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

/// Writes snapshot `id` from `parts`, one write each, and finishes it.
fn write(store: &GroupSnapshots, id: u64, parts: &[&[u8]]) -> TestResult<u64> {
    let mut sink = store.create(id)?;
    for part in parts {
        sink.write_all(part)?;
    }
    Ok(sink.finish()?)
}

fn read(store: &GroupSnapshots, id: u64) -> TestResult<Vec<u8>> {
    let mut source = store.open(id)?;
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    assert_eq!(source.length(), u64::try_from(bytes.len())?);
    Ok(bytes)
}

fn names(path: &Path) -> TestResult<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

#[test]
fn a_snapshot_written_as_a_stream_reads_back_whole() -> TestResult {
    let path = directory("round-trip")?;
    let store = GroupSnapshots::open(&path)?;
    let big = vec![9; 3 * 1024 * 1024 + 17];
    assert_eq!(
        write(&store, 7, &[b"head", &big, b"tail"])?,
        3 * 1024 * 1024 + 25
    );
    let mut expected = b"head".to_vec();
    expected.extend_from_slice(&big);
    expected.extend_from_slice(b"tail");
    assert_eq!(read(&store, 7)?, expected);
    // Finished, not adopted: readable, not the newest.
    assert_eq!(store.newest()?, None);
    store.adopt(7)?;
    assert_eq!(store.newest()?, Some(7));
    assert_eq!(read(&GroupSnapshots::open(&path)?, 7)?, expected);
    Ok(())
}

#[test]
fn a_snapshot_not_finished_leaves_nothing() -> TestResult {
    let path = directory("unfinished")?;
    let store = GroupSnapshots::open(&path)?;
    {
        let mut sink = store.create(3)?;
        sink.write_all(b"half")?;
    }
    assert!(store.open(3).is_err());
    assert!(store.adopt(3).is_err());
    assert_eq!(names(&path)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn adopting_keeps_one_and_any_finished_after_it() -> TestResult {
    let path = directory("adopt")?;
    let store = GroupSnapshots::open(&path)?;
    for id in [2, 3, 5, 9] {
        write(&store, id, &[format!("s{id}").as_bytes()])?;
    }
    store.adopt(2)?;
    store.adopt(5)?;
    // 2 was replaced and 3, finished before 5, is stale; 9, finished after
    // 5, is still to be installed.
    assert_eq!(store.newest()?, Some(5));
    assert!(store.open(2).is_err());
    assert!(store.open(3).is_err());
    assert_eq!(read(&store, 9)?, b"s9");
    // A restart forgets the finished one and keeps the adopted one.
    let reopened = GroupSnapshots::open(&path)?;
    assert!(reopened.open(9).is_err());
    assert_eq!(read(&reopened, 5)?, b"s5");
    assert_eq!(names(&path)?, vec!["00000000000000000005.gsnap".to_owned()]);
    Ok(())
}

#[test]
fn a_changed_byte_fails_the_read_that_reaches_the_end() -> TestResult {
    let path = directory("corrupt")?;
    let store = GroupSnapshots::open(&path)?;
    write(&store, 4, &[&vec![1; 64 * 1024]])?;
    store.adopt(4)?;
    let file = path.join("00000000000000000004.gsnap");
    let mut bytes = std::fs::read(&file)?;
    let middle = bytes.len() / 2;
    if let Some(byte) = bytes.get_mut(middle) {
        *byte ^= 0xff;
    }
    std::fs::write(&file, &bytes)?;
    let mut source = store.open(4)?;
    let mut read = Vec::new();
    let error = source.read_to_end(&mut read).err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(std::io::ErrorKind::InvalidData)
    );
    // A truncated file is refused when opened.
    std::fs::write(&file, bytes.get(..bytes.len() - 1).unwrap_or_default())?;
    assert!(store.open(4).is_err());
    Ok(())
}

#[test]
fn a_file_that_names_another_snapshot_or_format_is_refused() -> TestResult {
    let path = directory("header")?;
    let store = GroupSnapshots::open(&path)?;
    write(&store, 6, &[b"six"])?;
    store.adopt(6)?;
    // The same bytes under another id name the snapshot they were.
    std::fs::copy(
        path.join("00000000000000000006.gsnap"),
        path.join("00000000000000000008.gsnap"),
    )?;
    assert!(store.open(8).is_err());
    // Another format's magic bytes, at the same length.
    let file = path.join("00000000000000000006.gsnap");
    let mut bytes = std::fs::read(&file)?;
    if let Some(byte) = bytes.first_mut() {
        *byte ^= 0xff;
    }
    std::fs::write(&file, &bytes)?;
    assert!(store.open(6).is_err());
    Ok(())
}

/// A stopping member closes its store: its work still in flight (a
/// snapshot being written, one about to be adopted) and any that starts
/// later are refused and leave the directory as it was, so a member opened
/// on it next is not raced by the one that stopped.
#[test]
fn a_closed_store_leaves_its_directory_as_it_was() -> TestResult {
    let path = directory("closed")?;
    let store = GroupSnapshots::open(&path)?;
    write(&store, 3, &[b"adopted"])?;
    store.adopt(3)?;
    write(&store, 4, &[b"finished"])?;
    let mut sink = store.create(5)?;
    sink.write_all(b"in flight")?;
    let before = names(&path)?;
    store.close();
    assert!(sink.finish().is_err());
    assert!(store.adopt(4).is_err());
    assert!(store.create(6).is_err());
    assert_eq!(names(&path)?, before);
    assert_eq!(read(&store, 3)?, b"adopted");
    Ok(())
}
