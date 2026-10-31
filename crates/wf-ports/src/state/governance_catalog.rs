//! Bounded catalog observations; neither placement nor strong-read authority.

use wf_domain::workflow::request::RequestScope;

mod progress;
mod prune;
pub use prune::{
    GovernanceCatalogPrunePage, GovernanceCatalogPruneReadPort, encode_catalog_member_revision,
    inspect_catalog_member_revision,
};
mod reader;
pub use progress::{
    GovernanceCatalogProgress, GovernanceCatalogProgressPort, encode_catalog_progress_cursor,
    inspect_catalog_progress_cursor,
};
pub use reader::{GovernanceCatalogReadLimits, GovernanceCatalogReadPort, LoadedGovernanceRecord};

/// One exact namespace/catalog traversal request.
#[derive(Clone, Copy)]
pub struct GovernanceCatalogRead<'a> {
    /// Separately verified tenant/group/recovery namespace.
    pub scope: &'a RequestScope,
    /// Exact catalog bytes, independent of member identities.
    pub catalog: &'a [u8],
    /// Actual previous member, which must still exist to continue.
    pub cursor: Option<&'a [u8]>,
    /// At most256 members per coherent page.
    pub limit: u16,
}

/// Lexically ordered owned members from one coherent bounded read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceCatalogPage {
    /// Original member bytes, with at most256 owners.
    pub members: Vec<Vec<u8>>,
    /// A full page exposes its last member even if the next page will be empty.
    pub full: bool,
}
impl GovernanceCatalogPage {
    /// Borrows the last existing owner rather than cloning cursor payload bytes.
    #[must_use]
    pub fn next_cursor(&self) -> Option<&[u8]> {
        if self.full {
            self.members.last().map(Vec::as_slice)
        } else {
            None
        }
    }
}
