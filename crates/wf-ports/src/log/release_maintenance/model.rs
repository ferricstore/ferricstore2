//! One bounded command owner, moved apart from original retention header owners.
use super::C;
use wf_domain::workflow::{
    control::{EntryIdentity, PreparedDigest},
    request::{ClientIntent, RequestScope, gate::CommittedHeader},
};
use wf_types::{
    identity::RequestId,
    version::{CommittedTime, LogIdentity},
};

/// Completely decoded private maintenance frame; no current queue authority.
pub struct PreparedReleaseMaintenanceRequest<'a> {
    pub(super) encoded: &'a [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) intent: ClientIntent,
    pub(super) stamp: CommittedTime,
    pub(super) digest: PreparedDigest,
    pub(super) isolation: Vec<u8>,
    pub(super) command: C,
}
impl<'a> PreparedReleaseMaintenanceRequest<'a> {
    /// Original immutable donor bytes, without copying or a health permit.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }
    /// Exact original committed header; full prepared digest binds every byte.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: &self.scope,
            request: &self.request,
            intent: self.intent,
            stamp: self.stamp,
            identity: EntryIdentity::new(log, self.digest),
            encoded_entry: self.encoded,
            encoded_isolation_scope: &self.isolation,
        }
    }
    /// Complete original command; this borrow grants no live read authority.
    #[must_use]
    pub const fn command(&self) -> &C {
        &self.command
    }
    /// Moves original owners once, so effects may borrow the separated command spine.
    #[must_use]
    pub fn into_retained_parts(self) -> (RequestScope, RequestId, ClientIntent, C) {
        (self.scope, self.request, self.intent, self.command)
    }
}
