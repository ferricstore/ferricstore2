//! Nonbirth Claim companions borrow actual accepted plans and captured bindings.

use super::ClaimTransactionError;
use crate::state::redb::values::{
    binding,
    budget::{ValueCharge, product, total},
};
use crate::state::{RedbStateTransaction, value::ValueBindingData};
use crate::transaction::WorkflowValueTransactionLimits;
use std::mem::size_of;
use wf_domain::workflow::batch::ScopedPlan;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_ports::state::{GroupClaimChanges, GroupClaimTransaction, StateTransactionError as Error};

pub(super) struct ClaimValueResources {
    pub(super) limits: WorkflowValueTransactionLimits,
    charge: ValueCharge,
}

impl ClaimValueResources {
    pub(super) fn new(
        limits: WorkflowValueTransactionLimits,
        encoded: usize,
    ) -> Result<Self, ClaimTransactionError> {
        let mut charge = ValueCharge::new(
            limits.maximum_value_rows,
            limits.maximum_value_bytes,
            limits.maximum_value_work,
            limits.maximum_value_temporary_bytes,
        )?;
        charge.work(total(4096, [product(encoded, 16)?])?)?;
        charge.temporary(size_of::<Self>())?;
        Ok(Self { limits, charge })
    }
}

pub(crate) struct ClaimValueStep<'plan> {
    scoped: &'plan ScopedPlan,
    ordering: OrderingMetadata,
    binding: ValueBindingData,
}

impl ClaimValueStep<'_> {
    pub(crate) const fn parts(&self) -> (&ScopedPlan, OrderingMetadata, ValueBindingData) {
        (self.scoped, self.ordering, self.binding)
    }
}

pub(crate) struct ClaimValuePublicationPlan<'plan> {
    steps: Vec<ClaimValueStep<'plan>>,
    prepared: PreparedDigest,
    charge: ValueCharge,
}

impl ClaimValuePublicationPlan<'_> {
    pub(crate) const fn prepared(&self) -> PreparedDigest {
        self.prepared
    }
    pub(crate) fn split(&mut self) -> (&[ClaimValueStep<'_>], &mut ValueCharge) {
        (&self.steps, &mut self.charge)
    }
}

pub(super) fn commit(
    tx: RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    resources: Option<ClaimValueResources>,
) -> Result<(), ClaimTransactionError> {
    let Some(resources) = resources else {
        return Ok(tx.commit_claim(changes)?);
    };
    let mut publication = build(&tx, changes, resources)?;
    Ok(tx.commit_claim_values(changes, &mut publication)?)
}

pub(super) fn commit_governed(
    tx: RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    resources: ClaimValueResources,
    capture: wf_ports::log::CapturedGovernedClaimPayment<'_>,
) -> Result<(), ClaimTransactionError> {
    let mut publication = build(&tx, changes, resources)?;
    Ok(tx.commit_governed_claim_values(changes, &mut publication, capture)?)
}

fn build<'plan>(
    tx: &RedbStateTransaction<'_>,
    changes: &'plan GroupClaimChanges<'_, '_>,
    mut resources: ClaimValueResources,
) -> Result<ClaimValuePublicationPlan<'plan>, ClaimTransactionError> {
    resources.charge.helper()?;
    let count = changes
        .business()
        .map_or(0, |business| business.batch().items().len());
    let fixed = total(
        size_of::<ClaimValuePublicationPlan<'_>>(),
        [
            size_of::<ClaimValueResources>(),
            size_of::<ValueBindingData>(),
            size_of::<OrderingMetadata>(),
        ],
    )?;
    let mut steps = resources.charge.reserve(count, fixed)?;
    let live = total(fixed, [product(count, size_of::<ClaimValueStep<'_>>())?])?;
    let prepared = changes
        .plan()
        .control()
        .next()
        .last_entry()
        .ok_or(Error::InvalidChanges)?
        .prepared();
    if let Some(business) = changes.business() {
        let defaults = tx.value_manifest()?.defaults;
        for (scoped, ordering) in business.batch().items().iter().zip(business.metadata()) {
            resources.charge.candidate()?;
            let write = scoped.plan().write().ok_or(Error::InvalidChanges)?;
            let expected = write.expected_version.ok_or(Error::InvalidChanges)?;
            let mut binding = binding::load_at_version(
                tx,
                changes.scope(),
                (scoped.partition(), write.record.id()),
                expected,
                defaults,
                &mut resources.charge,
                live,
            )?;
            binding.lifecycle.effective_ttl_ms = None;
            binding.lifecycle.terminal_deadline = None;
            steps.push(ClaimValueStep {
                scoped,
                ordering,
                binding,
            });
        }
    }
    Ok(ClaimValuePublicationPlan {
        steps,
        prepared,
        charge: resources.charge,
    })
}
