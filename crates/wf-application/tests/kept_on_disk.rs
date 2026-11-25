//! A kept version on disk (ADR-016 phase D) is where its bytes are: two
//! copies are the same version when they name the same place, whichever
//! file handle each holds, and the last copy gives its file back once.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use wf_application::commands::flow::record::{Retained, StoredValue, ValueFiles};

#[derive(Default)]
struct Released(Mutex<Vec<u64>>);

impl ValueFiles for Released {
    fn release(&self, file: u64) {
        if let Ok(mut released) = self.0.lock() {
            released.push(file);
        }
    }
}

fn stored(location: (u64, u64, u32), files: &Arc<Released>) -> Retained {
    let files: Arc<dyn ValueFiles> = files.clone();
    Retained::Stored(Arc::new(StoredValue::new(location, files)))
}

#[test]
fn kept_versions_on_disk_are_equal_when_they_name_the_same_bytes() {
    let one = Arc::new(Released::default());
    let other = Arc::new(Released::default());
    let at = stored((7, 128, 832), &one);
    assert_eq!(at, stored((7, 128, 832), &other));
    assert_ne!(at, stored((8, 128, 832), &one), "another file");
    assert_ne!(at, stored((7, 129, 832), &one), "another offset");
    assert_ne!(at, stored((7, 128, 833), &one), "another length");
    assert_ne!(at, Retained::Bytes(vec![0; 832]));
    assert_eq!(
        format!("{at:?}"),
        "Stored(StoredValue { file: 7, offset: 128, length: 832, .. })"
    );
}

#[test]
fn the_last_copy_of_a_kept_version_on_disk_releases_its_file_once() -> Result<(), String> {
    let files = Arc::new(Released::default());
    let first = stored((7, 128, 832), &files);
    let copy = first.clone();
    drop(first);
    assert_eq!(released(&files)?, Vec::<u64>::new());
    drop(copy);
    assert_eq!(released(&files)?, vec![7]);
    Ok(())
}

fn released(files: &Released) -> Result<Vec<u64>, String> {
    files
        .0
        .lock()
        .map(|released| released.clone())
        .map_err(|error| error.to_string())
}
