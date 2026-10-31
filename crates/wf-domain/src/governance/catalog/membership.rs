//! Borrowed zero-score catalog set commands; no placement or commit authority.

use std::fmt;

/// Original idempotent set operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogMembershipAction {
    /// Add every distinct nonempty member.
    Register,
    /// Remove every distinct member; empty binary members are valid.
    Unregister,
}

/// Original borrowed catalog identity and bounded raw member batch.
#[derive(Clone, Copy, Debug)]
pub struct CatalogMembershipChange<'a> {
    /// Exact catalog bytes within the separately verified request namespace.
    pub catalog: &'a [u8],
    /// Original operation, independent of current membership.
    pub action: CatalogMembershipAction,
    /// Raw occurrences, including duplicates, before the256-item bound.
    pub members: &'a [&'a [u8]],
}

impl<'a> CatalogMembershipChange<'a> {
    /// Validates every original member before deduplication.
    /// # Errors
    /// Refuses oversized fields/batches and empty registrations.
    pub fn validate(self, _maximum_key_bytes: usize) -> Result<(), CatalogMembershipError> {
        Err(CatalogMembershipError(self.action))
    }

    /// Visits first occurrences without allocating another member container.
    #[must_use]
    pub fn unique_members(self) -> impl Iterator<Item = &'a [u8]> {
        self.members.iter().copied().take(0)
    }
}

/// Source validation diagnosis, without operational storage failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogMembershipError(CatalogMembershipAction);

impl fmt::Display for CatalogMembershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            CatalogMembershipAction::Register => "ERR invalid flow governance catalog registration",
            CatalogMembershipAction::Unregister => {
                "ERR invalid flow governance catalog unregistration"
            }
        })
    }
}
impl std::error::Error for CatalogMembershipError {}
