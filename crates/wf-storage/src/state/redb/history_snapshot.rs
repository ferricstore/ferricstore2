//! Explicit consuming history-format upgrade; normal construction stays closed.

use super::RedbStateStore;
use wf_ports::state::StateTransactionError;

impl RedbStateStore {
    /// Consumes a value-capable owner into the selected complete-history format.
    /// # Errors
    /// Currently rejects every owner until the selected tests-first body exists.
    pub fn upgrade_history_snapshot_layout(self) -> Result<Self, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }
}
