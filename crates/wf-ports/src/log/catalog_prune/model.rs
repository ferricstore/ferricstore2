//! Complete original header owners with catalog/cursor borrowed from the donor.
use wf_domain::{
    governance::catalog::CatalogPruneRequest,
    workflow::{
        control::{EntryIdentity, PreparedDigest},
        request::{ClientIntent, RequestScope, gate::CommittedHeader},
    },
};
use wf_types::{
    identity::RequestId,
    version::{CommittedTime, LogIdentity},
};

/// Fully checked private request; not current authority or proof of payment.
pub struct PreparedCatalogPruneRequest<'a> {
    pub(super) encoded: &'a [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) intent: ClientIntent,
    pub(super) digest: PreparedDigest,
    pub(super) isolation: Vec<u8>,
    pub(super) command: CatalogPruneRequest<'a>,
    pub(super) owned_bytes: usize,
}
impl<'a> PreparedCatalogPruneRequest<'a> {
    /// Borrows the unchanged original payload fields.
    #[must_use]
    pub const fn command(&self) -> CatalogPruneRequest<'a> {
        self.command
    }
    /// Borrows the verified original complete frame.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.encoded
    }
    /// Complete prospective original donor/header cohort, not payment proof.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
    /// Moves original scope/request owners without cloning payloads.
    #[must_use]
    pub fn into_retained_parts(
        self,
    ) -> (
        RequestScope,
        RequestId,
        ClientIntent,
        CatalogPruneRequest<'a>,
    ) {
        (self.scope, self.request, self.intent, self.command)
    }
    /// Binds explicit committed-log identity to the verified original bytes.
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
