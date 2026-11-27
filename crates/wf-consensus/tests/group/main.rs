//! ADR-018 phase 1: the adapter and fault gate. `OpenRaft`'s storage suite runs
//! against the entry log, and in-process groups over a test network that
//! loses messages and cuts links, with members stopped and restarted, keep
//! every committed entry.

#![forbid(unsafe_code)]

mod cluster;
mod machine;
mod memory;
mod network;
mod peer;
mod suite;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// A multi-thread Tokio runtime for `OpenRaft`.
fn runtime() -> TestResult<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .build()?)
}
