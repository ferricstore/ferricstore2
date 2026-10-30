//! Original public allocator, full raw authority, and immutable snapshot custody.
use super::TestResult;
mod activation;
mod faults;
mod fixtures;
pub(crate) use fixtures::public_frame as original_public_policy_frame;
mod interop;
mod limits;
mod replay;
mod roundtrip;
mod snapshots;
