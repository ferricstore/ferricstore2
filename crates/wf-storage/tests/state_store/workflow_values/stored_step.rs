//! Typed Step through actual named Claim, complete retained history and recovery.

use super::{TestResult, claim::fixtures as c, fixtures as v};
use crate::fixtures as f;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{Operation, StepContinue};
use wf_ports::consensus::{PreparedOperation, PreparedRequest};
use wf_ports::log::{PreparedBytes, encode_stored_step_continue};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    WorkflowValueTransactionError, apply_stored_step_continue_transaction,
};
use wf_types::identity::RequestId;
use wf_types::version::Fence;

mod fixtures;
mod limits;
mod recovery;
mod source;
