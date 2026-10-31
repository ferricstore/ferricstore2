//! Actual credit plans staged beneath the single original public Claim gate.
use super::{CreditReadContext, catalog, write::rows};
use crate::state::redb::{release::claimed, store::RedbStateTransaction};
use wf_domain::{
    governance::{
        credit::{CreditError, CreditLimits},
        identified::{
            IdentifiedCreditDecision as D, IdentifiedCreditMutation as M, IdentifiedCreditRequest,
            prepare_catalog_credit, prepare_identified_credit, resume_identified_credit,
        },
    },
    workflow::control::plan_control,
};
use wf_ports::{
    log::FreshAtomicClaim,
    state::{GroupApplyTransaction, StateTransactionError as E},
};

pub(crate) fn stage_claim_credit(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    request: IdentifiedCreditRequest<'_>,
    credit: CreditLimits,
    work: (&mut u64, u64),
) -> Result<D, E> {
    tx.checked(|| stage(tx, fresh, request, credit, work))
}

fn stage(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    request: IdentifiedCreditRequest<'_>,
    credit: CreditLimits,
    (used, maximum): (&mut u64, u64),
) -> Result<D, E> {
    tx.ready()?;
    tx.require_governed_claim()?;
    let root = fresh.prepared();
    let next = fresh.control().next();
    let identity = next.last_entry().ok_or(E::InvalidChanges)?;
    if root.intent().version() != 21
        || root.funding_batch().is_none()
        || root.scope().group() != tx.group
        || root.scope().recovery() != tx.recovery
        || identity.prepared() != root.digest()
        || plan_control(
            tx.control()?,
            identity,
            next.last_entry_stamp().ok_or(E::InvalidChanges)?,
        )
        .map_err(|_| E::InvalidChanges)?
            != fresh.control()
    {
        return Err(E::InvalidChanges);
    }
    let remaining = maximum
        .checked_sub(*used)
        .filter(|n| *n != 0)
        .ok_or(E::Limit)?;
    let credit = CreditLimits::new(
        credit.max_leases(),
        credit.max_reservations(),
        credit.max_owned_bytes(),
        credit.max_work().min(remaining),
    )
    .map_err(credit_error)?;
    let context = CreditReadContext::new(tx, root.scope(), request.scope, credit)?;
    if context.shards != request.shard_count {
        return Err(E::InvalidChanges);
    }
    let owner = context.owner()?;
    let mut pending = if owner.is_none()
        && context.catalog
        && request.configuration.is_some()
        && matches!(request.mutation, M::Lease { .. })
    {
        let (meta, tail) = catalog::observation(&context)?;
        prepare_catalog_credit(
            request,
            fresh.control().execution_time(),
            credit,
            wf_domain::governance::catalog::CreditCatalogObservation {
                meta,
                tail_scope: tail.as_deref(),
            },
        )
    } else {
        prepare_identified_credit(owner, request, fresh.control().execution_time(), credit)
    }
    .map_err(credit_error)?;
    while let Some(query) = pending.query() {
        let observed = context.observe(query)?;
        pending = resume_identified_credit(pending, observed).map_err(credit_error)?;
    }
    let plan = pending.finish().map_err(credit_error)?;
    *used = used
        .checked_add(plan.work_units())
        .filter(|n| *n <= maximum)
        .ok_or(E::Limit)?;
    catalog::publish(&context, &plan)?;
    claimed::retire(tx, root.scope(), &plan)?;
    rows::publish(&context, &plan)?;
    Ok(plan.decision())
}

const fn credit_error(error: CreditError) -> E {
    match error {
        CreditError::Allocation => E::Allocation,
        CreditError::InvalidState => E::Corrupt,
        CreditError::InvalidInput => E::InvalidChanges,
        CreditError::InvalidLimits | CreditError::Limit => E::Limit,
    }
}
