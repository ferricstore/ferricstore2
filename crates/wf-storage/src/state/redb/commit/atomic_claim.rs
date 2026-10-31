//! One actual writer stages every phase before its sole outcome/control commit.

use super::super::{
    admission::storage_error,
    store::{RedbStateTransaction, fence},
};
use super::{publication, validate_versions, write_control, write_outcome, write_records};
use crate::transaction::ClaimValuePublicationPlan;
use wf_domain::{
    claim::{
        apply::{ClaimDecision, ClaimRejection},
        coordination::ClaimPhase,
    },
    policy::complete::PolicySchemaCharge,
    workflow::{
        control::{ControlPlan, ControlState, plan_control},
        request::gate::{GateDecision, RequestGate},
        request::{RetainedMetadata, RetainedOutcome},
    },
};
use wf_ports::{
    log::{
        AtomicClaimReplyBuilder, FreshAtomicClaim, PreparedAtomicClaim,
        atomic_claim_retained_summary,
    },
    state::{GroupApplyTransaction, StateTransactionError as E},
};
use wf_types::version::TimestampMillis;
mod payment;

pub(crate) struct StagedAtomicClaim<'s, 'p, 'input> {
    pub(super) tx: RedbStateTransaction<'s>,
    pub(super) prepared: &'p PreparedAtomicClaim<'input>,
    pub(super) expected: ControlState,
    pub(super) control: ControlPlan,
    pub(super) reply: AtomicClaimReplyBuilder<'input>,
    pub(super) changed: usize,
    pub(super) reservation_cursor: usize,
    pub(super) initial_rejection: Option<ClaimRejection>,
    pub(super) limits: crate::transaction::AtomicClaimTransactionLimits,
}
impl<'s, 'p, 'input> StagedAtomicClaim<'s, 'p, 'input> {
    pub(crate) fn new(
        tx: RedbStateTransaction<'s>,
        expected: ControlState,
        fresh: &FreshAtomicClaim<'p, 'input>,
        reply_owned: usize,
        limits: crate::transaction::AtomicClaimTransactionLimits,
        schema: &mut PolicySchemaCharge,
    ) -> Result<Self, E> {
        let prepared = fresh.prepared();
        validate_guard(&tx, expected, prepared, fresh.control())?;
        let initial_rejection = payment::initial(
            &tx,
            prepared,
            fresh.control().execution_time(),
            limits,
            schema,
        )?;
        let reply = AtomicClaimReplyBuilder::new(prepared.payment(), tx.admission, reply_owned)
            .map_err(codec)?;
        Ok(Self {
            tx,
            prepared,
            expected,
            control: fresh.control(),
            reply,
            changed: 0,
            reservation_cursor: 0,
            initial_rejection,
            limits,
        })
    }
    pub(crate) const fn tx(&self) -> &RedbStateTransaction<'s> {
        &self.tx
    }
    pub(crate) const fn progress(&self) -> (usize, usize, usize) {
        self.reply.progress()
    }
    pub(crate) const fn initial_rejection(&self) -> Option<ClaimRejection> {
        self.initial_rejection
    }
    pub(crate) fn usage(&self) -> (usize, usize, usize) {
        (
            self.tx.steps.get(),
            self.tx.read.get(),
            self.tx.temporary.get(),
        )
    }
    pub(crate) fn stage(
        &mut self,
        kind: ClaimPhase,
        decision: &ClaimDecision<'_>,
        values: &mut ClaimValuePublicationPlan<'_>,
    ) -> Result<(), E> {
        let fenced = self.tx.fenced;
        self.stage_inner(kind, decision, values)
            .inspect_err(|error| fence(fenced, *error))
    }
    fn stage_inner(
        &mut self,
        kind: ClaimPhase,
        decision: &ClaimDecision<'_>,
        values: &mut ClaimValuePublicationPlan<'_>,
    ) -> Result<(), E> {
        validate_guard(&self.tx, self.expected, self.prepared, self.control)?;
        let ClaimDecision::Accepted(plan) = decision else {
            return Err(E::InvalidChanges);
        };
        let business = plan.batch();
        super::claim::values::validate_phase(&self.tx, self.prepared.digest(), business, values)?;
        let count = business.map_or(0, |batch| batch.batch().items().len());
        let verified = if let Some(paid) = self.reply.payment().map_err(codec)? {
            Some(super::super::release::claim::witness_atomic(
                &self.tx,
                self.prepared,
                paid,
                self.control.execution_time(),
                business,
                self.reservation_cursor,
                self.limits.credit,
            )?)
        } else {
            None
        };
        let mut positions = [0_u16; 1000];
        let used = if verified.is_some() {
            for (offset, position) in positions
                .get_mut(..count)
                .ok_or(E::Limit)?
                .iter_mut()
                .enumerate()
            {
                *position = u16::try_from(
                    self.reservation_cursor
                        .checked_add(offset)
                        .ok_or(E::Limit)?,
                )
                .map_err(|_| E::Limit)?;
            }
            positions.get(..count).ok_or(E::Limit)?
        } else {
            &[]
        };
        self.reply.push(kind, decision, used).map_err(codec)?;
        if let Some(business) = business {
            validate_versions(&self.tx, self.prepared.scope(), business)?;
            let lanes = publication::affected_lanes(
                &self.tx,
                business.deltas().len(),
                business.deltas().map(Ok),
            )?;
            for lane in &lanes {
                publication::head_due(&self.tx, self.prepared.scope(), *lane, false)?;
            }
            for delta in business.deltas() {
                publication::index_delta(&self.tx, self.prepared.scope(), &delta)?;
            }
            match verified.as_ref() {
                Some(verified) => {
                    payment::write_records(&self.tx, self.prepared.scope(), business, verified)?;
                }
                None => write_records(&self.tx, self.prepared.scope(), business, None)?,
            }
            for lane in lanes {
                publication::head_due(&self.tx, self.prepared.scope(), lane, true)?;
            }
            self.changed = self
                .changed
                .checked_add(business.batch().items().len())
                .ok_or(E::Limit)?;
            if let Some(paid) = self.reply.payment().map_err(codec)? {
                super::super::release::claim::publish_at(
                    &self.tx,
                    self.prepared.scope(),
                    business,
                    paid,
                    self.reservation_cursor,
                )?;
                self.reservation_cursor =
                    self.reservation_cursor.checked_add(count).ok_or(E::Limit)?;
            }
        }
        super::claim::values::publish(&self.tx, self.prepared.scope(), values)
    }
    pub(crate) fn abort(self) -> Result<(), E> {
        let fenced = self.tx.fenced;
        self.tx
            .write
            .abort()
            .map_err(|error| storage_error(&error))
            .inspect_err(|error| fence(fenced, *error))
    }
    pub(crate) fn finish(
        self,
        ttl: u64,
        rejection: Option<ClaimRejection>,
        schema: &mut PolicySchemaCharge,
    ) -> Result<(ControlState, Box<RetainedOutcome>), E> {
        validate_guard(&self.tx, self.expected, self.prepared, self.control)?;
        if rejection.is_some() && self.changed != 0 {
            return Err(E::InvalidChanges);
        }
        if rejection == Some(ClaimRejection::GoverningReservationUnavailable)
            && payment::initial(
                &self.tx,
                self.prepared,
                self.control.execution_time(),
                self.limits,
                schema,
            )? != rejection
        {
            return Err(E::InvalidChanges);
        }
        let time = self.control.execution_time();
        let response = match rejection {
            Some(reason) => self.reply.finish_rejected(reason),
            None => self.reply.finish_accepted(),
        }
        .map_err(codec)?;
        let (status, _) = atomic_claim_retained_summary(
            self.prepared.intent(),
            17,
            &response,
            time,
            self.tx.admission,
        )
        .map_err(codec)?;
        let log = self
            .control
            .next()
            .last_entry()
            .ok_or(E::InvalidChanges)?
            .log();
        let outcome = Box::new(
            RetainedOutcome::new(
                RetainedMetadata {
                    scope: self.prepared.scope().clone(),
                    request: self.prepared.request().clone(),
                    intent: self.prepared.intent(),
                    original_position: log,
                    original_time: time,
                    expires_at: TimestampMillis::new(time.get().checked_add(ttl).ok_or(E::Limit)?),
                    status,
                },
                17,
                response,
                self.tx.admission,
            )
            .map_err(|_| E::InvalidChanges)?,
        );
        let control = self.control.next();
        let fenced = self.tx.fenced;
        publish(self.tx, self.prepared, control, Some(&outcome))
            .inspect_err(|error| fence(fenced, *error))?;
        Ok((control, outcome))
    }
}

pub(crate) fn complete(
    tx: RedbStateTransaction<'_>,
    expected: ControlState,
    prepared: &PreparedAtomicClaim<'_>,
    gate: RequestGate<'_>,
) -> Result<ControlState, E> {
    validate_guard(&tx, expected, prepared, gate.control())?;
    let fenced = tx.fenced;
    if gate.decision() == GateDecision::LogReplay {
        tx.write
            .abort()
            .map_err(|error| storage_error(&error))
            .inspect_err(|error| fence(fenced, *error))?;
    } else {
        publish(tx, prepared, gate.control().next(), None)
            .inspect_err(|error| fence(fenced, *error))?;
    }
    Ok(gate.control().next())
}

pub(super) fn validate_guard(
    tx: &RedbStateTransaction<'_>,
    expected: ControlState,
    prepared: &PreparedAtomicClaim<'_>,
    control: ControlPlan,
) -> Result<(), E> {
    tx.ready()?;
    tx.require_governed_claim()?;
    if prepared.scope().group() != tx.group
        || prepared.scope().recovery() != tx.recovery
        || tx.control()? != expected
    {
        return Err(E::KnownAborted);
    }
    let next = control.next();
    let identity = next.last_entry().ok_or(E::InvalidChanges)?;
    let stamp = next.last_entry_stamp().ok_or(E::InvalidChanges)?;
    if identity.prepared() != prepared.digest()
        || plan_control(expected, identity, stamp).map_err(|_| E::InvalidChanges)? != control
    {
        return Err(E::InvalidChanges);
    }
    Ok(())
}

pub(super) fn publish(
    mut tx: RedbStateTransaction<'_>,
    prepared: &PreparedAtomicClaim<'_>,
    control: ControlState,
    outcome: Option<&RetainedOutcome>,
) -> Result<(), E> {
    if let Some(outcome) = outcome {
        write_outcome(&tx, prepared.scope(), outcome)?;
    }
    write_control(&tx, control)?;
    tx.write
        .set_durability(redb::Durability::Immediate)
        .map_err(|_| E::Unavailable)?;
    tx.write.commit().map_err(|_| E::Uncertain)
}
pub(super) fn codec(error: wf_ports::log::CodecError) -> E {
    match error {
        wf_ports::log::CodecError::OwnedLimit
        | wf_ports::log::CodecError::ByteLimit
        | wf_ports::log::CodecError::ItemLimit
        | wf_ports::log::CodecError::ValueLimit => E::Limit,
        wf_ports::log::CodecError::Allocation => E::Allocation,
        _ => E::InvalidChanges,
    }
}
