//! Explicit target membership selection; no manufactured service commitment.

use super::CommittedRecoveryLimits;

#[derive(Clone, Copy)]
pub(super) enum GovernanceCatalogRecoveryProfile {
    Disabled,
    Enabled,
}

impl CommittedRecoveryLimits {
    /// Explicitly enables target catalog bodies on the normal apply profile.
    #[must_use]
    pub const fn with_governance_catalogs(mut self) -> Self {
        self.governance_catalog = GovernanceCatalogRecoveryProfile::Enabled;
        self
    }
}
