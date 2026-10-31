//! Verified original byte binding; no transaction, routing or payment authority.

use wf_domain::governance::catalog::CreditCatalogAckRequest;
use wf_domain::workflow::{
    control::{EntryIdentity, PreparedDigest},
    request::{ClientIntent, RequestScope, gate::CommittedHeader},
};
use wf_types::{
    identity::RequestId,
    version::{CommittedTime, LogIdentity},
};

/// Complete prepared private frame, borrowing its exact original donor.
pub struct PreparedCatalogAckRequest<'a> {
    pub(super) encoded: &'a [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) intent: ClientIntent,
    pub(super) digest: PreparedDigest,
    pub(super) isolation: Vec<u8>,
    pub(super) command: CreditCatalogAckRequest,
    pub(super) owned_bytes: usize,
}

impl PreparedCatalogAckRequest<'_> {
    /// Moves original neutral fields after the fresh gate, without payload clones.
    /// This transfer supplies no admission, current-state or commit authority.
    #[must_use]
    pub fn into_retained_parts(
        self,
    ) -> (
        RequestScope,
        RequestId,
        ClientIntent,
        CreditCatalogAckRequest,
    ) {
        (self.scope, self.request, self.intent, self.command)
    }

    /// Borrows the exact verified original bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.encoded
    }
    /// Reads the original immutable acknowledgement controls.
    #[must_use]
    pub const fn command(&self) -> CreditCatalogAckRequest {
        self.command
    }
    /// Reads the prospectively bounded decoder cohort, not paid authority.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
    /// Binds the assigned log identity to the verified original fields.
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
