//! Original catalog scenarios use actual committed binary set membership.
use super::*;
use wf_application::governance::{
    GovernanceCatalogCollection, GovernanceCatalogCollectionError,
    GovernanceCatalogCollectionLimits, GovernanceCatalogCollectionRequest,
    collect_governance_catalog,
};
use wf_domain::{
    governance::catalog::{CatalogMembershipAction, CatalogMembershipChange},
    workflow::request::RetainedStatus,
};
use wf_ports::{log::encode_catalog_membership_request, state::LoadedGovernanceRecord};

impl PublicationFixture {
    fn catalog_stamp(&self) -> TestResult<(u64, CommittedTime)> {
        let (mut log, mut state, _) = self.open(&self.target)?;
        let index = log.state()?.last_log().map_or(0, |id| id.index().get() + 1);
        let tx = state.begin_apply(
            self.target.group(),
            self.target.recovery(),
            f::driver_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?;
        let time = tx
            .control()?
            .time()
            .hlc()
            .physical()
            .get()
            .checked_add(1)
            .ok_or("fixture clock exhausted")?;
        Ok((index, f::clock(time, 0, time)?))
    }
    pub(in crate::catalog_publication) fn change_catalog_members(
        &self,
        catalog: &[u8],
        action: CatalogMembershipAction,
        members: &[&[u8]],
    ) -> TestResult<Vec<RetainedStatus>> {
        let mut replies = Vec::new();
        for chunk in members.chunks(256) {
            let (index, stamp) = self.catalog_stamp()?;
            let request = RequestId::new(format!("catalog-fixture-{index}").into_bytes())?;
            let frame = encode_catalog_membership_request(
                &self.target,
                &request,
                stamp,
                CatalogMembershipChange {
                    catalog,
                    action,
                    members: chunk,
                },
                f::driver_limits().codec,
                16 * f::MIB,
            )?;
            let outcome = self.commit(&self.target, &request, frame)?;
            assert_eq!(outcome.scope(), &self.target);
            assert_eq!(outcome.request(), &request);
            assert_eq!(outcome.response(), b"FWGR\0\x01");
            replies.push(outcome.status());
        }
        Ok(replies)
    }
    pub(in crate::catalog_publication) fn collect_raw_catalog(
        &self,
        catalog: &[u8],
        prefix: &[u8],
        limit: usize,
        mut on_load: impl FnMut(&[u8]) -> Result<(), S>,
    ) -> Result<GovernanceCatalogCollection<Vec<u8>>, GovernanceCatalogCollectionError> {
        collect_governance_catalog(
            self,
            GovernanceCatalogCollectionRequest {
                scope: &self.target,
                catalog,
                limit,
            },
            |key, limits| {
                on_load(key)?;
                if !key.starts_with(prefix) {
                    return Ok(None);
                }
                let owned_bytes = std::mem::size_of::<Vec<u8>>()
                    .checked_add(key.len())
                    .ok_or(S::Limit)?;
                if owned_bytes > limits.maximum_bytes {
                    return Err(S::Limit);
                }
                let mut record = Vec::new();
                record
                    .try_reserve_exact(key.len())
                    .map_err(|_| S::Allocation)?;
                record.extend_from_slice(key);
                Ok(Some(LoadedGovernanceRecord {
                    record,
                    owned_bytes,
                }))
            },
            |a, b| b.cmp(a),
            GovernanceCatalogCollectionLimits::default(),
        )
    }
}
