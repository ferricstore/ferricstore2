//! Actual consuming schema upgrade and atomic Step storage; source parity stays open.

use super::{TestResult, fixtures as values};
use crate::fixtures as common;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{Operation, StartAndClaim, StepContinue};
use wf_ports::consensus::{PreparedOperation, PreparedRequest};
use wf_ports::log::{PreparedBytes, encode_step_continue, step_continue_retained_summary};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord,
};
use wf_storage::state::{RedbStateStore, StateStoreLimits, key, value};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, WorkflowValueTransactionError, WorkflowValueTransactionLimits,
    apply_step_continue_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

mod faults;
mod fixtures;
mod limits;
mod publication;
mod replay;
mod snapshot;
mod upgrade;
