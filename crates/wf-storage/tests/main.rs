//! Every integration test of `wf-storage` in one binary, so the suite compiles
//! and links once (ADR-012). Each module is an earlier test target, loaded
//! unchanged from its own file.

#![forbid(unsafe_code)]

#[path = "history_snapshot/main.rs"]
mod history_snapshot;
#[path = "record_batch_transaction/main.rs"]
mod record_batch_transaction;
#[path = "record_capture/main.rs"]
mod record_capture;
#[path = "state_keys/main.rs"]
mod state_keys;
#[path = "state_values/main.rs"]
mod state_values;

mod cold_files;
mod snapshot_files;
