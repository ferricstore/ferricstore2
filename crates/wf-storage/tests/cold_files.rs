//! Cold files for finished Flows (ADR-016): images read back by location
//! and checked, files deleted when sealed and no stub points into them,
//! and files no stub names removed after a restart.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use wf_storage::cold::{ColdError, ColdStore, Location};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn directory(tag: &str) -> TestResult<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "wf-cold-{}-{}-{tag}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&path)?;
    Ok(path.join("cold"))
}

#[test]
fn images_read_back_by_location_from_any_writer() -> TestResult {
    let path = directory("roundtrip")?;
    let store = ColdStore::open(&path)?;
    let mut first = store.writer()?;
    let mut second = store.writer()?;
    let a = first.append(b"image-a")?;
    let b = second.append(b"image-b")?;
    let c = first.append(b"")?;
    first.flush()?;
    second.flush()?;
    assert_ne!(a.file, b.file);
    assert_eq!(store.read(a)?, b"image-a");
    assert_eq!(store.read(b)?, b"image-b");
    assert_eq!(store.read(c)?, b"");
    assert_eq!(c.offset, 15, "the second record follows the first");
    Ok(())
}

#[test]
fn a_damaged_or_wrong_location_is_refused() -> TestResult {
    let path = directory("corrupt")?;
    let store = ColdStore::open(&path)?;
    let mut writer = store.writer()?;
    let location = writer.append(b"payload-bytes")?;
    writer.sync()?;
    let wrong = Location {
        length: location.length + 1,
        ..location
    };
    assert!(matches!(store.read(wrong), Err(ColdError::Corrupt)));
    let file = path.join(format!("{:020}.cold", location.file));
    let mut bytes = std::fs::read(&file)?;
    if let Some(last) = bytes.last_mut() {
        *last ^= 0xFF;
    }
    std::fs::write(&file, &bytes)?;
    let reopened = ColdStore::open(&path)?;
    assert!(matches!(reopened.read(location), Err(ColdError::Corrupt)));
    Ok(())
}

#[test]
fn a_sealed_file_goes_with_its_last_stub_and_not_before() -> TestResult {
    let path = directory("release")?;
    let store = ColdStore::open(&path)?;
    let mut writer = store.writer()?;
    let file = writer.file();
    let first = writer.append(b"one")?;
    writer.append(b"two")?;
    store.hold(file, 2);
    store.release(file, 1)?;
    writer.seal()?;
    assert_eq!(store.files()?, vec![file], "one stub still points in");
    assert_eq!(store.read(first)?, b"one");
    store.release(file, 1)?;
    assert!(store.files()?.is_empty(), "sealed with no stub left");
    // An open writer's file stays even with no stub.
    let open = store.writer()?;
    assert_eq!(store.files()?, vec![open.file()]);
    Ok(())
}

#[test]
fn after_a_restart_files_are_sealed_and_unnamed_ones_removed() -> TestResult {
    let path = directory("restart")?;
    let (kept, dropped) = {
        let store = ColdStore::open(&path)?;
        let mut first = store.writer()?;
        let mut second = store.writer()?;
        let kept = first.append(b"kept")?;
        let dropped = second.append(b"dropped")?;
        first.sync()?;
        second.sync()?;
        (kept, dropped)
    };
    let store = ColdStore::open(&path)?;
    let fresh = store.writer()?;
    assert!(
        fresh.file() > dropped.file.max(kept.file),
        "ids never reused"
    );
    store.hold(kept.file, 1);
    store.remove_unreferenced(&HashSet::from([kept.file]))?;
    assert_eq!(store.files()?, vec![kept.file, fresh.file()]);
    assert_eq!(store.read(kept)?, b"kept");
    store.release(kept.file, 1)?;
    assert_eq!(store.files()?, vec![fresh.file()]);
    Ok(())
}
