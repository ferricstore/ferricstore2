//! Named Claim with actual immutable values, captured history and source ordering.

use super::{TestResult, fixtures as v};
use crate::fixtures as f;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_ports::log::PreparedBytes;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionOutcome, WorkflowValueTransactionLimits, apply_workflow_value_claim_transaction,
};

mod atomic;
mod chain;
mod faults;
pub(super) mod fixtures;
mod governed;
mod limits;
mod public;
mod replay;
mod retention;
