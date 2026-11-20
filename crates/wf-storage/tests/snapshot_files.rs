//! Snapshot files (ADR-013): written whole or not at all, read back only when
//! the checksum holds, and removed oldest first.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wf_storage::snapshot::files::{SnapshotError, SnapshotFiles};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn directory(tag: &str) -> TestResult<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-snapshot-files-{}-{}-{tag}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

fn names(path: &Path) -> TestResult<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(path)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<TestResult<_>>()?;
    names.sort();
    Ok(names)
}

#[test]
fn the_newest_snapshot_reads_back_whole() -> TestResult {
    let root = directory("newest")?;
    let path = root.join("snapshots");
    let files = SnapshotFiles::open(&path)?;
    assert_eq!(files.newest()?, None);
    files.write(1, &[b"first"])?;
    files.write(2, &[b"sec", b"", b"ond"])?;
    assert_eq!(files.ids()?, vec![1, 2]);
    assert_eq!(files.newest()?, Some((2, b"second".to_vec())));
    let reopened = SnapshotFiles::open(&path)?;
    assert_eq!(reopened.newest()?, Some((2, b"second".to_vec())));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_damaged_or_misnamed_newest_snapshot_is_refused() -> TestResult {
    let root = directory("damaged")?;
    let files = SnapshotFiles::open(&root)?;
    files.write(1, &[b"older"])?;
    files.write(2, &[b"newest body"])?;
    let newest = root.join(names(&root)?.last().ok_or("no file")?);
    let mut bytes = std::fs::read(&newest)?;
    let middle = bytes.len() / 2;
    if let Some(byte) = bytes.get_mut(middle) {
        *byte ^= 0x40;
    }
    std::fs::write(&newest, &bytes)?;
    assert_eq!(files.newest(), Err(SnapshotError::Corrupt { id: 2 }));
    bytes.truncate(10);
    std::fs::write(&newest, &bytes)?;
    assert_eq!(files.newest(), Err(SnapshotError::Corrupt { id: 2 }));
    std::fs::remove_file(&newest)?;
    let first = root.join(names(&root)?.first().ok_or("no file")?);
    std::fs::rename(
        &first,
        first.with_file_name(
            names(&root)?
                .first()
                .ok_or("no file")?
                .replacen('1', "7", 1),
        ),
    )?;
    assert_eq!(files.newest(), Err(SnapshotError::Corrupt { id: 7 }));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_leftover_temp_file_is_ignored_and_removed() -> TestResult {
    let root = directory("temp")?;
    let files = SnapshotFiles::open(&root)?;
    files.write(3, &[b"kept"])?;
    let name = names(&root)?.pop().ok_or("no file")?;
    std::fs::write(root.join(format!("{name}.tmp")), b"half written")?;
    std::fs::write(root.join("unrelated.txt"), b"left alone")?;
    let reopened = SnapshotFiles::open(&root)?;
    assert_eq!(reopened.ids()?, vec![3]);
    assert_eq!(reopened.newest()?, Some((3, b"kept".to_vec())));
    assert_eq!(names(&root)?, vec![name, "unrelated.txt".to_owned()]);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn missing_parent_directories_are_created() -> TestResult {
    let root = directory("nested")?;
    let files = SnapshotFiles::open(&root.join("data").join("snapshots"))?;
    files.write(1, &[b"x"])?;
    assert_eq!(files.ids()?, vec![1]);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn removal_keeps_the_named_snapshot_and_newer() -> TestResult {
    let root = directory("remove")?;
    let files = SnapshotFiles::open(&root)?;
    for id in 1..=4 {
        files.write(id, &[b"x"])?;
    }
    files.remove_before(3)?;
    assert_eq!(files.ids()?, vec![3, 4]);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
