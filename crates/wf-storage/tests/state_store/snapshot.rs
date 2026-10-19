//! Complete immutable snapshot authority, independent wire bytes and real faults.

use std::collections::BTreeMap;

use super::{TestResult, fixtures as f};
use wf_ports::snapshot::{SnapshotError, SnapshotLimits, SnapshotMetadata, StateSnapshotStore};
use wf_storage::snapshot::RedbSnapshotStage;

mod corruption;
mod faults;
mod fixtures;
mod lifecycle;
mod limits;

type RawCohort = BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>;
