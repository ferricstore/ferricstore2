//! Consumed workflow value transaction boundary and explicit logical limits.

use std::fmt;

use wf_domain::workflow::request::RequestScope;
use wf_types::version::LogIdentity;

use crate::state::RedbStateStore;

use super::{
    RecordBatchTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
};

/// Additional publication ceilings, carried with the existing driver profile.
pub struct WorkflowValueTransactionLimits {
    /// Existing decoder, policy, staging and adapter admission profile.
    pub record_batch: RecordBatchTransactionLimits,
    /// Maximum admitted value-related rows.
    pub maximum_value_rows: usize,
    /// Maximum cumulative admitted value bytes.
    pub maximum_value_bytes: usize,
    /// Maximum cumulative charged value work.
    pub maximum_value_work: usize,
    /// Maximum simultaneous logical temporary value ownership.
    pub maximum_value_temporary_bytes: usize,
}

/// Operational refusal; never retained as a fabricated business outcome.
#[derive(Debug)]
pub enum WorkflowValueTransactionError {
    /// Existing record driver refusal.
    Record(RecordBatchTransactionError),
    /// Prepared inputs violate the selected semantic profile.
    InvalidInput,
    /// Required captured policy reference is absent.
    PolicyReferenceRequired,
    /// The selected policy generation has not reached this authority.
    PolicyGenerationNotApplied,
    /// Current authority has advanced beyond the prepared generation.
    StalePolicyGeneration,
    /// Equal generations have different retained policy backing.
    ConflictingPolicyGeneration,
    /// The prepared target no longer identifies the current birth.
    StalePolicyTarget,
    /// This operation or persistent profile is not yet supported.
    UnsupportedProfile,
}

impl fmt::Display for WorkflowValueTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Record(error) => fmt::Display::fmt(error, formatter),
            Self::InvalidInput => formatter.write_str("invalid workflow value input"),
            Self::PolicyReferenceRequired => formatter.write_str("policy reference required"),
            Self::PolicyGenerationNotApplied => {
                formatter.write_str("policy generation not applied")
            }
            Self::StalePolicyGeneration => formatter.write_str("stale policy generation"),
            Self::ConflictingPolicyGeneration => {
                formatter.write_str("conflicting policy generation")
            }
            Self::StalePolicyTarget => formatter.write_str("stale policy target"),
            Self::UnsupportedProfile => formatter.write_str("unsupported workflow value profile"),
        }
    }
}

impl std::error::Error for WorkflowValueTransactionError {}

/// Applies one prepared value command through the existing consumed driver.
///
/// # Errors
/// Refuses unsupported workflow value profiles before publication.
pub fn apply_workflow_value_batch_transaction(
    _store: &mut RedbStateStore,
    _scope: &RequestScope,
    _encoded: &[u8],
    _log: LogIdentity,
    _limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    Err(WorkflowValueTransactionError::UnsupportedProfile)
}
