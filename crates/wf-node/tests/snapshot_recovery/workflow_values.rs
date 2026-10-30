//! Real operation-eight startup composition, never a service or catalog seed.

use super::{TestResult, disk_backend, f};
use wf_node::recovery::{
    CommittedRecoveryError as Error, CommittedRecoveryLimits as Limits, recover_committed,
    recover_snapshotted,
};

mod audit;
mod claim;
mod faults;
mod fixtures;
mod guards;
mod independent;
mod limits;
mod replay;
mod snapshot;
mod step_continue;
mod stored;
