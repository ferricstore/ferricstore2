//! Explicit server clock boundary for coherent workflow value reads.

use wf_types::version::{HlcTimestamp, TimestampMillis};

use crate::state::StateTransactionError;

/// Samples the server's HLC and wall time together for one eligible read batch.
pub trait WorkflowValueReadClock {
    /// Returns one coherent pair; the caller must not resample per reference.
    ///
    /// # Errors
    /// Returns operational clock failure without converting it to missing data.
    fn read_snapshot(&mut self) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError>;
}
