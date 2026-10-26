//! Explicit workflow-value recovery profile, independent of legacy opt-ins.

use wf_storage::transaction::WorkflowValueTransactionLimits;

use super::{CommittedRecoveryError, CommittedRecoveryLimits};

impl CommittedRecoveryLimits {
    /// Selects additional publication ceilings for value-entry recovery.
    /// # Errors
    /// Refuses unsupported workflow-value recovery.
    pub fn with_workflow_value_limits(
        self,
        _rows: usize,
        _bytes: usize,
        _work: usize,
        _temporary: usize,
    ) -> Result<Self, CommittedRecoveryError> {
        Err(CommittedRecoveryError::UnsupportedEntry)
    }

    /// Reads the value profile; legacy construction leaves it absent.
    #[must_use]
    pub const fn workflow_value_limits(self) -> Option<WorkflowValueTransactionLimits> {
        match self.workflow_values {
            Some([rows, bytes, work, temporary]) => Some(WorkflowValueTransactionLimits {
                record_batch: self.normal,
                maximum_value_rows: rows,
                maximum_value_bytes: bytes,
                maximum_value_work: work,
                maximum_value_temporary_bytes: temporary,
            }),
            None => None,
        }
    }
}
