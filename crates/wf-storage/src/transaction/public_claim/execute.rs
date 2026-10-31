//! The existing actual phase engine runs only after definitive successful funding.
use super::{ClaimTransactionError, PublicClaimTransactionLimits as Limits};
use crate::{
    state::redb::{FundingResult, StagedAtomicClaim},
    transaction::claim::values::ClaimValueResources,
};
use wf_domain::{claim::apply::ClaimRejection, policy::complete::PolicySchemaCharge};
use wf_ports::log::FreshAtomicClaim;

pub(super) fn run<'s, 'p, 'input>(
    fresh: &FreshAtomicClaim<'p, 'input>,
    staged: StagedAtomicClaim<'s, 'p, 'input>,
    funding: &FundingResult,
    limits: Limits,
    resources: ClaimValueResources,
    schema: &mut PolicySchemaCharge,
) -> Result<(StagedAtomicClaim<'s, 'p, 'input>, Option<ClaimRejection>), ClaimTransactionError> {
    if matches!(funding, FundingResult::Rejected(_)) {
        return Ok((staged, None));
    }
    crate::transaction::atomic_claim::execute::run(fresh, staged, limits.base, resources, schema)
}
