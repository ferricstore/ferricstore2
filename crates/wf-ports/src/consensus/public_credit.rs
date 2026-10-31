//! Authorized public credit inquiry, preparation and supervised immutable submission.
use super::{CreditCatalogPublicationPort, MutationNotSubmitted, MutationSubmission};
use crate::{
    log::PreparedBytes,
    state::{CreditCatalogReadLimits, CreditOwnerReadPort, GovernanceCatalogReadPort},
};
use std::sync::Arc;
use wf_domain::{
    governance::caller::NormalizedCreditCall,
    workflow::request::{ClientIntent, RequestScope},
};
use wf_types::{identity::RequestId, version::CommittedTime};
/// Original request inquiry; Fresh never substitutes for the applied request gate.
pub enum CreditRequestLookup {
    /// No current retained identity was observed under the authorized lifecycle.
    Fresh,
    /// Original authority or a definitive/unresolved inquiry diagnosis.
    Resolved(MutationSubmission),
}
/// Actual current authorization, placement/read lifecycle and supervised execution.
/// Retain a shared frame and execution admission through actual completion even
/// after Unknown/cancellation. Immutable bytes confer no execution authority.
pub trait PublicCreditSubmitPort:
    CreditCatalogPublicationPort + CreditOwnerReadPort + GovernanceCatalogReadPort
{
    /// Resolves identity/conflict/retention before fresh source reads or entropy.
    /// # Errors
    /// Refuses current authorization/routing/read admission before CREDIT proposal.
    fn lookup_public_credit(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        intent: ClientIntent,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditRequestLookup, MutationNotSubmitted>;
    /// Supplies trusted time and a fresh16-byte batch only for public Spend.
    /// # Errors
    /// Refuses before CREDIT proposal; prior catalog repair may already commit.
    fn prepare_public_credit(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        caller: NormalizedCreditCall<'_>,
    ) -> Result<(CommittedTime, Option<[u8; 16]>), MutationNotSubmitted>;
    /// Holds shared original bytes/admission until actual supervised completion.
    fn submit_public_credit(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        frame: Arc<PreparedBytes>,
        workspace: usize,
    ) -> MutationSubmission;
}
