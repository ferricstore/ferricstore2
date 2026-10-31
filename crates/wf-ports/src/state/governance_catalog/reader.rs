//! Configured bounded read capabilities; values carry no routing authority.

use super::{GovernanceCatalogPage, GovernanceCatalogRead};
use crate::state::StateTransactionError;
use wf_domain::workflow::request::RequestScope;

/// Positive cumulative limits for one coherent read/loader guard.
#[derive(Clone, Copy, Debug)]
pub struct GovernanceCatalogReadLimits {
    /// Maximum cumulative open/probe/advance steps.
    pub maximum_steps: usize,
    /// Complete cumulative logical decoded/returned owner bytes.
    pub maximum_bytes: usize,
    /// Complete cumulative key/adapter staging bytes.
    pub maximum_temporary_bytes: usize,
}
impl Default for GovernanceCatalogReadLimits {
    fn default() -> Self {
        Self {
            maximum_steps: 10000,
            maximum_bytes: 8 * 1_048_576,
            maximum_temporary_bytes: 8 * 1_048_576,
        }
    }
}

/// A moved loader value with its complete logical ownership charge.
/// The configured loader must admit actual allocation before constructing it;
/// this scalar is a contract check, not an admission/payment certificate.
pub struct LoadedGovernanceRecord<T> {
    /// Original record; collecting never requires Clone.
    pub record: T,
    /// Complete fixed and dynamic logical bytes owned by this record.
    pub owned_bytes: usize,
}

/// Configured namespace and current read lifecycle belong to this adapter.
/// Reads have independent coherent revisions; no strong-read barrier is implied.
pub trait GovernanceCatalogReadPort {
    /// Reads one original anchored page, with at most256 binary members.
    /// # Errors
    /// Refuses namespace/capability/cursor, corruption, lifecycle or admission.
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, StateTransactionError>;

    /// Looks up one exact member under a bounded coherent revision.
    /// # Errors
    /// Refuses namespace/capability, corruption, lifecycle or admission.
    fn governance_catalog_contains(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, StateTransactionError>;
}
