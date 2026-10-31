//! Bounded queue observations; no release, acknowledgement or freshness proof.
use crate::log::{CodecError, inspect_release_intent};
use wf_domain::governance::release::{ReleaseIntentRef, ReleaseOutboxState};

/// One original native intent owner and its same-revision completion marker.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkflowReleaseEntry {
    /// Positive original sequence within the observed pending range.
    pub sequence: u64,
    /// Sparse completion observation; no acknowledgement authority.
    pub completed: bool,
    encoded: Vec<u8>,
}
impl WorkflowReleaseEntry {
    /// Checks complete original bytes before taking their existing owner.
    /// # Errors
    /// Refuses malformed fields/EOF or an invalid sequence.
    pub fn new(sequence: u64, completed: bool, encoded: Vec<u8>) -> Result<Self, CodecError> {
        if !(1..=wf_domain::workflow::MAX_EXACT_MILLIS).contains(&sequence) {
            return Err(CodecError::Malformed);
        }
        inspect_release_intent(&encoded, encoded.len())?;
        Ok(Self {
            sequence,
            completed,
            encoded,
        })
    }
    /// Borrows the original complete bytes without copying any payload.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    /// Inspects a borrowed original intent; no identity is regenerated.
    /// # Errors
    /// Refuses malformed original bytes, with the same complete codec checks.
    pub fn intent(&self) -> Result<ReleaseIntentRef<'_>, CodecError> {
        inspect_release_intent(&self.encoded, self.encoded.len())
    }
}

/// At most256 consecutive entries from one observed queue revision.
pub struct WorkflowReleasePage {
    /// Same-revision finite scalar queue state.
    pub meta: ReleaseOutboxState,
    /// Original ascending sequence and byte owners, including completed entries.
    pub entries: Vec<WorkflowReleaseEntry>,
    /// More pending sequences follow this page in that same revision.
    pub more: bool,
}
