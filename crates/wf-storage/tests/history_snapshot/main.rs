//! Versioned snapshot data, not storage publication or full source acceptance.

#![forbid(unsafe_code)]

mod fixtures;
mod limits;
mod profiles;
mod refusals;
mod retry;
mod wire;

use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{CommittedCommand, Operation, StepContinue, WorkflowRecord, reduce};
use wf_ports::state::{StoredWorkflowHistorySnapshot, StoredWorkflowRecord};
use wf_storage::state::value::{
    HistorySnapshotLimits, StateValueError, decode_history, decode_history_snapshot,
    encode_history_snapshot, encode_record_fields, encode_record_snapshot, scan_record_point,
    scan_record_snapshot,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const WORKSPACE: usize = 1_048_576;
