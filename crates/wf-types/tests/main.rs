//! Every integration test of `wf-types` in one binary, so the suite compiles
//! and links once (ADR-012). Each module is an earlier test target, loaded
//! unchanged from its own file.

#![forbid(unsafe_code)]

#[path = "identity_and_bounds.rs"]
mod identity_and_bounds;
#[path = "partition_key.rs"]
mod partition_key;
#[path = "scope_and_committed_time.rs"]
mod scope_and_committed_time;
#[path = "version.rs"]
mod version;
