//! Positive immutable logical limits; no claimed native allocation bound.

use super::StateValueError;
use wf_domain::workflow::admission::AdmissionLimits;

/// Independent encoded, returned-owner and workspace ceilings for this codec.
#[derive(Clone, Copy)]
pub struct HistorySnapshotLimits {
    pub(super) admission: AdmissionLimits,
    pub(super) encoded: usize,
    pub(super) owned: usize,
    pub(super) workspace: usize,
}

impl HistorySnapshotLimits {
    /// Captures positive limits without treating them as data or authority facts.
    /// # Errors
    /// Rejects any zero encoded, owned or workspace ceiling.
    pub const fn new(
        admission: AdmissionLimits,
        encoded: usize,
        owned: usize,
        workspace: usize,
    ) -> Result<Self, StateValueError> {
        if encoded == 0 || owned == 0 || workspace == 0 {
            return Err(StateValueError::InvalidValue);
        }
        Ok(Self {
            admission,
            encoded,
            owned,
            workspace,
        })
    }

    /// Reads the immutable represented-field admission profile.
    #[must_use]
    pub const fn admission(self) -> AdmissionLimits {
        self.admission
    }
    /// Reads the complete envelope byte ceiling.
    #[must_use]
    pub const fn maximum_encoded_bytes(self) -> usize {
        self.encoded
    }
    /// Reads the complete logical returned-owner ceiling.
    #[must_use]
    pub const fn maximum_owned_bytes(self) -> usize {
        self.owned
    }
    /// Reads the simultaneous logical workspace ceiling.
    #[must_use]
    pub const fn maximum_workspace_bytes(self) -> usize {
        self.workspace
    }
}
