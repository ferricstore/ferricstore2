//! Snapshot provenance, pin-before-purge ordering and real committed replay.

#![forbid(unsafe_code)]

mod advance;
mod atomic_start;
mod claim;
mod core_policy;
#[path = "../../../wf-storage/tests/state_store/faults/backend.rs"]
mod disk_backend;
mod faults;
#[path = "../recovery/fixtures/shared.rs"]
mod fixture_data;
mod fixtures;
mod limits;
mod original_policy;
mod provenance;
mod replay;
mod workflow_values;

use fixture_data as f;
use wf_node::recovery::{
    CommittedRecoveryError, CommittedRecoveryLimits, SnapshotRecoveryError as Error,
    purge_snapshot, recover_snapshotted,
};
use wf_ports::log::RaftLogStore;
use wf_ports::snapshot::{SnapshotError, SnapshotLimits, StateSnapshotStore};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn caps() -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        wf_ports::consensus::RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        f::driver_limits(),
        (1_000, 64 * f::MIB),
        16 * f::MIB,
        512 * f::MIB,
    )?)
}
