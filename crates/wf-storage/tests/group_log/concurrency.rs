//! Concurrent appenders share syncs and every acknowledged record is kept
//! exactly once.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::thread;

use wf_storage::log::{GroupLog, GroupLogLimits};

use super::{TestResult, directory};

const THREADS: usize = 8;
const PER_THREAD: usize = 250;

fn worker(log: &GroupLog, thread_index: usize) -> TestResult<Vec<Vec<u8>>> {
    let mut acknowledged = Vec::with_capacity(PER_THREAD);
    for index in 0..PER_THREAD {
        let body = format!("t{thread_index}-{index}").into_bytes();
        let sequence = log.append(body.clone())?;
        log.wait_durable(sequence)?;
        acknowledged.push(body);
    }
    Ok(acknowledged)
}

#[test]
fn concurrent_appends_share_syncs_and_all_survive_reopen() -> TestResult {
    let directory = directory("concurrent")?;
    let path = directory.join("node.log");
    let mut acknowledged = BTreeSet::new();
    let syncs = {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        let log = Arc::new(log);
        let handles: Vec<_> = (0..THREADS)
            .map(|thread_index| {
                let log = Arc::clone(&log);
                thread::spawn(move || worker(&log, thread_index).map_err(|error| error.to_string()))
            })
            .collect();
        for handle in handles {
            let records = handle.join().map_err(|_| "appender panicked")??;
            acknowledged.extend(records);
        }
        log.sync_count()
    };
    let total = THREADS * PER_THREAD;
    assert_eq!(acknowledged.len(), total);
    assert!(syncs > 0, "no sync was recorded");
    assert!(
        usize::try_from(syncs)? < total,
        "{syncs} syncs for {total} waited appends: nothing was grouped"
    );
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(
        recovered.records.len(),
        total,
        "a record was lost or duplicated"
    );
    let replayed: BTreeSet<_> = recovered.records.into_iter().collect();
    assert_eq!(replayed, acknowledged);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
