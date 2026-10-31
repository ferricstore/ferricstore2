//! Real source/target reads behind the selected neutral capability contracts.
use super::*;
use wf_application::governance::{
    CreditCatalogRepairError, CreditCatalogRepairStatus, GovernanceCatalogCollection,
    GovernanceCatalogCollectionLimits as CollectionLimits, GovernanceCatalogCollectionRequest,
    collect_governance_catalog, collect_governance_keys, repair_credit_catalog,
};
use wf_domain::governance::credit::{CreditOperation, CreditOwner, decide_credit};
use wf_ports::state::{
    CreditOwnerReadPort, GovernanceCatalogPage, GovernanceCatalogReadLimits,
    GovernanceCatalogReadPort, LoadedGovernanceRecord, decode_credit_catalog_member,
    encode_credit_catalog_members,
};
use wf_types::version::TimestampMillis;

impl GovernanceCatalogReadPort for PublicationFixture {
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, S> {
        if self.fault == PublicationFault::UnavailablePage {
            return Err(S::Unavailable);
        }
        let (_, state, _) = self.open(request.scope).map_err(|_| S::Unavailable)?;
        state.governance_catalog_page(request, limits)
    }
    fn governance_catalog_contains(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, S> {
        let (_, state, _) = self.open(scope).map_err(|_| S::Unavailable)?;
        state.governance_catalog_contains(scope, catalog, member, limits)
    }
}
impl CreditOwnerReadPort for PublicationFixture {
    fn credit_catalog_owner(
        &self,
        scope: &RequestScope,
        credit_scope: &[u8],
        limits: CreditCatalogReadLimits,
    ) -> Result<Option<CreditOwner>, S> {
        let (_, state, _) = self.open(scope).map_err(|_| S::Unavailable)?;
        state.credit_catalog_owner(scope, credit_scope, limits)
    }
}
impl PublicationFixture {
    fn load_collection_owner(
        &self,
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<Option<LoadedGovernanceRecord<CreditOwner>>, S> {
        let member = decode_credit_catalog_member(member, 65535).map_err(|_| S::Corrupt)?;
        let source = self
            .sources
            .iter()
            .find(|source| {
                source.group().as_bytes() == member.group
                    && source.recovery().as_bytes() == member.recovery
                    && source.tenant() == self.target.tenant()
            })
            .ok_or(S::NamespaceConflict)?;
        let Some(owner) = self.credit_catalog_owner(
            source,
            member.credit_scope,
            CreditCatalogReadLimits {
                maximum_steps: limits.maximum_steps,
                maximum_bytes: limits.maximum_bytes,
                maximum_temporary_bytes: limits.maximum_temporary_bytes,
                ..CreditCatalogReadLimits::default()
            },
        )?
        else {
            return Ok(None);
        };
        let owner = decide_credit(
            owner,
            CreditOperation::ReclaimExpired {
                now: TimestampMillis::new(1001),
            },
            CreditLimits::default(),
        )
        .map_err(|_| S::Corrupt)?
        .into_parts()
        .0;
        if owner
            .view()
            .leases
            .iter()
            .any(|lease| !lease.reservations.is_empty())
        {
            return Err(S::Corrupt);
        }
        let owned_bytes = owner
            .view()
            .leases
            .len()
            .checked_mul(std::mem::size_of::<
                wf_domain::governance::credit::CreditLeaseData,
            >())
            .and_then(|n| n.checked_add(owner.view().scope.len()))
            .and_then(|n| n.checked_add(std::mem::size_of::<CreditOwner>() + 128))
            .ok_or(S::Limit)?;
        Ok(Some(LoadedGovernanceRecord {
            record: owner,
            owned_bytes,
        }))
    }
    pub(in crate::catalog_publication) fn collect_owners(
        &self,
        limit: usize,
    ) -> TestResult<GovernanceCatalogCollection<CreditOwner>> {
        Ok(collect_governance_catalog(
            self,
            GovernanceCatalogCollectionRequest {
                scope: &self.target,
                catalog: b"limit",
                limit,
            },
            |key, limits| self.load_collection_owner(key, limits),
            |a, b| a.view().scope.cmp(&b.view().scope),
            CollectionLimits::default(),
        )?)
    }
    pub(in crate::catalog_publication) fn collect_exact_owners(
        &self,
        index: usize,
        names: &[&[u8]],
        limit: usize,
    ) -> TestResult<GovernanceCatalogCollection<CreditOwner>> {
        let encoded =
            encode_credit_catalog_members(&self.sources[index], names, 16 * f::MIB, 16 * f::MIB)?;
        let keys = encoded.members().collect::<Vec<_>>();
        Ok(collect_governance_keys(
            &keys,
            limit,
            |key, limits| self.load_collection_owner(key, limits),
            |a, b| a.view().scope.cmp(&b.view().scope),
            CollectionLimits::default(),
        )?)
    }
    pub(in crate::catalog_publication) fn repair(
        &mut self,
        index: usize,
        name: &[u8],
        mut limits: Limits,
    ) -> Result<CreditCatalogRepairStatus, CreditCatalogRepairError> {
        limits.execution_workspace_bytes = 512 * f::MIB;
        let source = self.sources[index].clone();
        let target = self.target.clone();
        repair_credit_catalog(self, &source, &target, b"limit", name, limits)
    }
}
