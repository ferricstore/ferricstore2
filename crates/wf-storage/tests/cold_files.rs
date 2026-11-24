//! Cold files for finished Flows (ADR-016): images read back by location
//! and checked, files deleted when sealed and no stub points into them,
//! and files no stub names removed after a restart.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use wf_storage::cold::{ColdBatch, ColdError, ColdQueue, ColdStore, Location};

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
    store.remove_unreferenced(&HashSet::from([kept.file]), &HashSet::new())?;
    assert_eq!(store.files()?, vec![kept.file, fresh.file()]);
    assert_eq!(store.read(kept)?, b"kept");
    store.release(kept.file, 1)?;
    assert_eq!(store.files()?, vec![fresh.file()]);
    Ok(())
}

#[test]
fn a_file_filled_within_one_writer_round_keeps_its_images() -> TestResult {
    // A full file is sealed before the next image; images appended in the
    // same round are not counted until the round's sync, so a file holding
    // only them read as unused and was deleted at its seal.
    let path = directory("filled")?;
    let store = ColdStore::open(&path)?;
    let queue = ColdQueue::spawn_sized(&store, 16)?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let images: Vec<(u64, Vec<u8>)> = (1..=3_u8).map(|byte| (u64::MAX, vec![byte; 32])).collect();
    queue.send(ColdBatch {
        images: images.clone(),
        done: Box::new(move |located| drop(sender.send(located))),
    });
    let locations = receiver.recv()??;
    assert_eq!(locations.len(), 3);
    for (location, (_, image)) in locations.iter().zip(&images) {
        assert_eq!(&store.read(*location)?, image, "{location:?}");
    }
    Ok(())
}

#[test]
fn an_unused_file_waits_for_a_snapshot_begun_after_it() -> TestResult {
    // Replay from the last snapshot reads the files it names.
    let path = directory("snapshots")?;
    let store = ColdStore::open(&path)?;
    store.keep_for_snapshots();
    let mut early = store.writer()?;
    let mut late = store.writer()?;
    let (early_file, late_file) = (early.file(), late.file());
    let kept = early.append(b"early")?;
    late.append(b"late")?;
    store.hold(early_file, 1);
    store.hold(late_file, 1);
    early.seal()?;
    late.seal()?;
    store.release(early_file, 1)?;
    assert_eq!(store.read(kept)?, b"early", "kept until a snapshot");
    let first = store.snapshot_begins().ok_or("snapshots are kept for")?;
    store.release(late_file, 1)?;
    store.snapshot_written(first)?;
    assert_eq!(store.files()?, vec![late_file], "the capture may name it");
    let second = store.snapshot_begins().ok_or("snapshots are kept for")?;
    store.snapshot_written(second)?;
    assert_eq!(store.files()?, Vec::<u64>::new());
    Ok(())
}

#[test]
fn a_restart_keeps_the_files_the_loaded_snapshot_names_for_the_next() -> TestResult {
    let path = directory("named")?;
    let (named, unnamed) = {
        let store = ColdStore::open(&path)?;
        let mut first = store.writer()?;
        let mut second = store.writer()?;
        let named = first.append(b"named")?;
        let unnamed = second.append(b"unnamed")?;
        first.sync()?;
        second.sync()?;
        (named, unnamed)
    };
    let store = ColdStore::open(&path)?;
    store.keep_for_snapshots();
    store.hold(named.file, 1);
    let loaded = store.held();
    store.release(named.file, 1)?;
    store.remove_unreferenced(&HashSet::new(), &loaded)?;
    assert_eq!(store.files()?, vec![named.file], "{unnamed:?} is gone");
    assert_eq!(store.read(named)?, b"named");
    let begun = store.snapshot_begins().ok_or("snapshots are kept for")?;
    store.snapshot_written(begun)?;
    assert_eq!(store.files()?, Vec::<u64>::new());
    Ok(())
}
