//! Complete Claim plans and captured companions share the consumed writer.

use super::super::super::store::RedbStateTransaction;
use crate::transaction::ClaimValuePublicationPlan;
use wf_domain::history::EventKind;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GroupClaimChanges, StateTransactionError as Error};

pub(super) fn validate(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    publication: &mut ClaimValuePublicationPlan<'_>,
) -> Result<(), Error> {
    if tx.layout != 8 {
        return Err(Error::UnsupportedLayout);
    }
    if changes
        .plan()
        .control()
        .next()
        .last_entry()
        .is_none_or(|entry| entry.prepared() != publication.prepared())
    {
        return Err(Error::InvalidChanges);
    }
    validate_phase(tx, publication.prepared(), changes.business(), publication)
}

pub(in crate::state::redb::commit) fn validate_phase(
    tx: &RedbStateTransaction<'_>,
    prepared: wf_domain::workflow::control::PreparedDigest,
    business: Option<&wf_domain::workflow::indexed::IndexedBatchPlan<'_>>,
    publication: &mut ClaimValuePublicationPlan<'_>,
) -> Result<(), Error> {
    if tx.layout != 8 || publication.prepared() != prepared {
        return Err(Error::InvalidChanges);
    }
    let (steps, charge) = publication.split();
    charge.helper()?;
    let count = business.map_or(0, |business| business.batch().items().len());
    if steps.len() != count {
        return Err(Error::InvalidChanges);
    }
    if let Some(business) = business {
        for (step, (scoped, ordering)) in steps
            .iter()
            .zip(business.batch().items().iter().zip(business.metadata()))
        {
            charge.candidate()?;
            let (proposed, selected, binding) = step.parts();
            let write = scoped.plan().write().ok_or(Error::InvalidChanges)?;
            let expected = write.expected_version.ok_or(Error::InvalidChanges)?;
            if !core::ptr::eq(proposed, scoped)
                || selected != ordering
                || scoped
                    .plan()
                    .history()
                    .is_none_or(|event| event.kind != EventKind::Claimed)
                || binding
                    .versions
                    .iter()
                    .flatten()
                    .any(|version| *version > expected)
                || binding.lifecycle.effective_ttl_ms.is_some()
                || binding.lifecycle.terminal_deadline.is_some()
            {
                return Err(Error::InvalidChanges);
            }
        }
    }
    Ok(())
}

pub(in crate::state::redb::commit) fn publish(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    publication: &mut ClaimValuePublicationPlan<'_>,
) -> Result<(), Error> {
    let (steps, charge) = publication.split();
    charge.helper()?;
    let live = charge.peak();
    for step in steps {
        charge.candidate()?;
        let (scoped, _, binding) = step.parts();
        super::super::values::publish_binding_fields(
            tx, scope, scoped, binding, false, charge, live,
        )?;
        super::super::values::publish_binding_fields(
            tx, scope, scoped, binding, true, charge, live,
        )?;
    }
    Ok(())
}
