//! One bounded semantic transaction driver; concrete engines remain external.

use std::fmt;

use wf_domain::policy::complete::{CompletePolicyEncodingError, PolicySchemaLimits};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::log::{CodecLimits, RecordBatchApplyError};
use wf_ports::state::StateTransactionError;

mod claim;
pub(crate) use claim::values::ClaimValuePublicationPlan;
mod core_policy;
mod credit;
pub use credit::{CreditTransactionError, apply_credit_transaction};
mod original_policy;
pub use core_policy::{CorePolicyTransactionError, apply_complete_core_policy_patch_transaction};
pub use original_policy::{
    OriginalPolicyTransactionError, apply_original_policy_set_transaction,
    with_original_policy_get_reply,
};
mod load;
mod observations;
mod record_batch;
mod value_batch;
pub use value_batch::apply_stored_step_continue_transaction;
pub(crate) use value_batch::{ValuePublicationPlan, ValueStep};

pub use value_batch::{
    WorkflowValueTransactionError, WorkflowValueTransactionLimits, apply_step_continue_transaction,
    apply_workflow_stored_value_batch_transaction, apply_workflow_value_batch_transaction,
};

pub use claim::{
    ClaimTransactionError, ClaimTransactionLimits, ClaimTransactionOutcome,
    apply_claim_transaction, apply_workflow_value_claim_transaction,
};

pub use record_batch::{
    apply_atomic_start_transaction, apply_independent_record_batch_transaction,
    apply_public_mutation_transaction, apply_record_batch_transaction,
};

/// Explicit per-call logical profile, separate from native cache/pages and RSS.
///
/// All numeric fields must be positive. R/S/C are cumulative without refunds;
/// their configured sum is reserved before the prepared decoder creates owners.
#[derive(Clone, Copy)]
pub struct RecordBatchTransactionLimits {
    /// Actual private command/admission profile.
    pub codec: CodecLimits,
    /// Continuous complete-policy work profile for every reached type.
    pub schema: PolicySchemaLimits,
    /// Maximum transaction probes, opens, candidates and ends.
    pub maximum_read_steps: usize,
    /// R: cumulative admitted returned owners and policy-decode scratch.
    pub maximum_read_bytes: usize,
    /// S: independent query owners, intermediate slots, maps and projections.
    pub maximum_staging_bytes: usize,
    /// C: cumulative adapter scope/key/value/bound/position ownership.
    pub maximum_adapter_bytes: usize,
    /// W: simultaneous logical algorithm owners plus configured R/S/C.
    pub workspace_bytes: usize,
    /// Positive original outcome guarantee, added with checked arithmetic.
    pub outcome_ttl_ms: u64,
}

/// Operational driver error; never an encoded or retained business diagnosis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordBatchTransactionError {
    /// A required positive profile value is zero.
    InvalidLimits,
    /// Prepared data, retained gate, pure apply or workspace refusal.
    Apply(RecordBatchApplyError),
    /// Coherent authority, admission, allocation or durable completion failed.
    Transaction(StateTransactionError),
    /// Complete-policy validation, projection or continuous work refused.
    Policy(CompletePolicyEncodingError),
}

impl fmt::Display for RecordBatchTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid record transaction limits"),
            Self::Apply(error) => fmt::Display::fmt(error, formatter),
            Self::Transaction(error) => fmt::Display::fmt(error, formatter),
            Self::Policy(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for RecordBatchTransactionError {}
impl From<RecordBatchApplyError> for RecordBatchTransactionError {
    fn from(error: RecordBatchApplyError) -> Self {
        Self::Apply(error)
    }
}
impl From<StateTransactionError> for RecordBatchTransactionError {
    fn from(error: StateTransactionError) -> Self {
        Self::Transaction(error)
    }
}
impl From<CompletePolicyEncodingError> for RecordBatchTransactionError {
    fn from(error: CompletePolicyEncodingError) -> Self {
        Self::Policy(error)
    }
}

/// Owned acknowledgement/classification, returned only after consumed commit.
///
/// No input, transaction, policy, query or local predecessor borrow can escape.
#[derive(Debug, Eq, PartialEq)]
pub enum RecordBatchTransactionOutcome {
    /// New accepted/rejected outcome or the moved original replay metadata.
    Retained {
        /// Current processing checkpoint, independently of original reply time.
        control: ControlState,
        /// Complete original response/metadata, moved without payload cloning.
        outcome: Box<RetainedOutcome>,
    },
    /// Existing request reused with different caller intent; old outcome remains.
    IntentConflict(ControlState),
    /// Original request expired without executing again.
    Expired(ControlState),
    /// Exact last entry; no durable write.
    LogReplay(ControlState),
}
