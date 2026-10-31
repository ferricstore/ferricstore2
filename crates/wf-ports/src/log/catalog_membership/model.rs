//! Original neutral fields with member payloads borrowed from the verified frame.

use wf_domain::{
    governance::catalog::{CatalogMembershipAction, CatalogMembershipChange},
    workflow::{
        control::{EntryIdentity, PreparedDigest},
        request::{ClientIntent, RequestScope, gate::CommittedHeader},
    },
};
use wf_types::{
    identity::RequestId,
    version::{CommittedTime, LogIdentity},
};

/// Complete original private request; not current-state or paid authority.
pub struct PreparedCatalogMembershipRequest<'a> {
    pub(super) encoded: &'a [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) intent: ClientIntent,
    pub(super) digest: PreparedDigest,
    pub(super) isolation: Vec<u8>,
    pub(super) catalog: &'a [u8],
    pub(super) action: CatalogMembershipAction,
    pub(super) members: Vec<&'a [u8]>,
    pub(super) owned_bytes: usize,
}
impl<'a> PreparedCatalogMembershipRequest<'a> {
    /// Reads original mathematical command fields without copying payloads.
    #[must_use]
    pub fn command(&self) -> CatalogMembershipChange<'_> {
        CatalogMembershipChange {
            catalog: self.catalog,
            action: self.action,
            members: &self.members,
        }
    }
    /// Borrows the verified exact donor.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.encoded
    }
    /// Prospectively bounded decoder cohort, without payment provenance.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
    /// Moves original header owners and member references without payload clones.
    #[must_use]
    pub fn into_retained_parts(
        self,
    ) -> (
        RequestScope,
        RequestId,
        ClientIntent,
        &'a [u8],
        CatalogMembershipAction,
        Vec<&'a [u8]>,
    ) {
        (
            self.scope,
            self.request,
            self.intent,
            self.catalog,
            self.action,
            self.members,
        )
    }
    /// Binds a committed log position to these verified original fields.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: &self.scope,
            request: &self.request,
            intent: self.intent,
            identity: EntryIdentity::new(log, self.digest),
            stamp: self.stamp,
            encoded_entry: self.encoded,
            encoded_isolation_scope: &self.isolation,
        }
    }
}
