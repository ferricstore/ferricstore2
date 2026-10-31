//! Existing-owner repair must succeed before a dependent public mutation.
use super::{CreditCatalogPublicationError, CreditCatalogPublicationLimits};
use std::fmt;
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    consensus::CreditCatalogPublicationPort,
    state::{CreditOwnerReadPort, GovernanceCatalogReadPort, StateTransactionError},
};

/// Observed repair result; none grants later placement or mutation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogRepairStatus {
    /// No owner exists; no ghost member was created.
    Missing,
    /// The existing owner already has exact membership.
    Present,
    /// A complete accepted retained registration was confirmed.
    Registered,
}
/// Repair refusal; callers must not execute a dependent mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogRepairError {
    /// Whole-call source/target/resource preflight failed.
    Preflight(CreditCatalogPublicationError),
    /// Exact owner or member read failed.
    Read(StateTransactionError),
    /// Registration was refused, uncertain or returned an invalid original reply.
    Unconfirmed,
}
impl fmt::Display for CreditCatalogRepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preflight(e) => e.fmt(f),
            Self::Read(e) => e.fmt(f),
            Self::Unconfirmed => f.write_str("catalog repair registration unconfirmed"),
        }
    }
}
impl std::error::Error for CreditCatalogRepairError {}

/// Observes one exact owner and repairs its catalog entry through supervised ports.
/// Missing owners never cause registration. Only a complete accepted reply
/// permits a caller to continue; all public parents must still revalidate state.
/// # Errors
/// Refuses invalid preflight/read state or any unconfirmed registration.
pub fn repair_credit_catalog<P>(
    port: &mut P,
    source: &RequestScope,
    target: &RequestScope,
    catalog: &[u8],
    credit_scope: &[u8],
    limits: CreditCatalogPublicationLimits,
) -> Result<CreditCatalogRepairStatus, CreditCatalogRepairError>
where
    P: CreditCatalogPublicationPort + CreditOwnerReadPort + GovernanceCatalogReadPort,
{
    use CreditCatalogRepairError as E;
    use CreditCatalogRepairStatus as Status;
    use wf_ports::state::{GovernanceCatalogReadLimits, encode_credit_catalog_members};
    if credit_scope.is_empty() || credit_scope.len() > 65535 {
        return Err(E::Preflight(CreditCatalogPublicationError::InvalidOptions));
    }
    super::publication::plan::select(
        super::CreditCatalogPublicationRequest {
            sources: std::slice::from_ref(source),
            source_cursor: 0,
            target,
            catalog,
        },
        limits,
    )
    .map_err(E::Preflight)?;
    let owner = port
        .credit_catalog_owner(source, credit_scope, limits.read)
        .map_err(E::Read)?;
    let exists = owner.is_some();
    drop(owner);
    if !exists {
        return Ok(Status::Missing);
    }
    let encoded = encode_credit_catalog_members(
        source,
        &[credit_scope],
        limits.codec.admission().decision_bytes(),
        limits.maximum_members_bytes,
    )
    .map_err(|error| {
        E::Preflight(match error {
            wf_ports::log::CodecError::Allocation => CreditCatalogPublicationError::Allocation,
            _ => CreditCatalogPublicationError::Limit,
        })
    })?;
    let contains = port
        .governance_catalog_contains(
            target,
            catalog,
            encoded.members().next().ok_or(E::Unconfirmed)?,
            GovernanceCatalogReadLimits {
                maximum_steps: limits.read.maximum_steps,
                maximum_bytes: limits.read.maximum_bytes,
                maximum_temporary_bytes: limits.read.maximum_temporary_bytes,
            },
        )
        .map_err(E::Read)?;
    if contains {
        return Ok(Status::Present);
    }
    if super::publication::dispatch::encoded_members(port, target, catalog, &encoded, limits) {
        Ok(Status::Registered)
    } else {
        Err(E::Unconfirmed)
    }
}
