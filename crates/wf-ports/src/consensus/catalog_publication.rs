//! Internal maintenance capability; selection never manufactures a Raft service.

use super::{MutationNotSubmitted, MutationSubmission};
use crate::{log::PreparedBytes, state::CreditCatalogReadPort};
use wf_domain::workflow::request::RequestScope;
use wf_types::{identity::RequestId, version::CommittedTime};

/// Actual source owners and supervised internal publication/ACK submission.
/// Verify configured namespace, placement and maintenance authorization at both
/// stages. Retain bytes and admission through completion after handoff, even on
/// Unknown/cancellation. No-proposal proof is distinct from physical abort.
pub trait CreditCatalogPublicationPort: CreditCatalogReadPort {
    /// Prepares a trusted maintenance stamp at or after the explicit tick floor.
    /// # Errors
    /// Refuses before proposal when trusted time cannot meet that floor.
    fn prepare_catalog_command_at(
        &mut self,
        scope: &RequestScope,
        not_before_ms: u64,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted> {
        if not_before_ms > wf_domain::workflow::MAX_EXACT_MILLIS {
            return Err(MutationNotSubmitted::Preparation);
        }
        let prepared = self.prepare_catalog_command(scope)?;
        if prepared.1.wall().get() < not_before_ms {
            return Err(MutationNotSubmitted::Preparation);
        }
        Ok(prepared)
    }
    /// Prepares a fresh internal identity/stamp to avoid expired-attempt deadlock.
    /// # Errors
    /// Refuses before this attempt is proposed.
    fn prepare_catalog_command(
        &mut self,
        scope: &RequestScope,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted>;
    /// Consumes original complete private bytes under a supervised owner budget.
    /// Exact log replay must be resolved to retained authority or reported Unknown.
    fn submit_catalog_command(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        prepared: PreparedBytes,
        workspace_limit: usize,
    ) -> MutationSubmission;
}
