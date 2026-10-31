//! Public funding and settlement share the actual atomic Claim writer and gate.
use super::atomic_claim::{StagedAtomicClaim, codec, validate_guard};
use crate::{
    state::redb::store::RedbStateTransaction, transaction::PublicClaimTransactionLimits as Limits,
};
use wf_domain::{
    claim::apply::ClaimRejection,
    policy::complete::PolicySchemaCharge,
    workflow::{control::ControlState, request::RetainedOutcome},
};
use wf_ports::{
    log::{
        AtomicClaimReplyBuilder, FreshAtomicClaim, encode_public_claim_funding_reply,
        encode_public_claim_reply,
    },
    state::StateTransactionError as E,
};
mod funding;
mod presence;
mod receipt;
mod settlement;
pub(crate) use funding::FundingResult;

pub(crate) fn start<'s, 'p, 'input>(
    tx: RedbStateTransaction<'s>,
    expected: ControlState,
    fresh: &FreshAtomicClaim<'p, 'input>,
    reply_owned: usize,
    limits: Limits,
    schema: &mut PolicySchemaCharge,
    work: &mut u64,
) -> Result<(StagedAtomicClaim<'s, 'p, 'input>, FundingResult), E> {
    let root = fresh.prepared();
    validate_guard(&tx, expected, root, fresh.control())?;
    if root.intent().version() != 21 || root.funding_batch().is_none() || root.payment().is_some() {
        return Err(E::InvalidChanges);
    }
    let funding = tx.checked(|| funding::initialize(&tx, fresh, limits, schema, work))?;
    let admission = limits.base.values.record_batch.codec.admission();
    let reply = match &funding {
        FundingResult::Paid(paid) => AtomicClaimReplyBuilder::new_owned_payment(
            paid.encode(limits, reply_owned)?,
            admission,
            reply_owned,
        ),
        _ => AtomicClaimReplyBuilder::new(None, admission, reply_owned),
    }
    .map_err(codec)?;
    let staged = StagedAtomicClaim {
        tx,
        prepared: root,
        expected,
        control: fresh.control(),
        reply,
        changed: 0,
        reservation_cursor: 0,
        initial_rejection: None,
        limits: limits.base,
    };
    Ok((staged, funding))
}

pub(crate) fn finish(
    staged: StagedAtomicClaim<'_, '_, '_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    funding: FundingResult,
    limits: Limits,
    rejection: Option<ClaimRejection>,
    work: &mut u64,
) -> Result<(ControlState, Box<RetainedOutcome>), E> {
    validate_guard(&staged.tx, staged.expected, staged.prepared, staged.control)?;
    if fresh.prepared().digest() != staged.prepared.digest()
        || (rejection.is_some() && staged.changed != 0)
    {
        return Err(E::InvalidChanges);
    }
    let admission = limits.base.values.record_batch.codec.admission();
    let time = staged.control.execution_time();
    let maximum = limits.base.values.record_batch.maximum_staging_bytes;
    let response = match funding {
        FundingResult::Rejected(reason) => {
            let no_demand = staged
                .tx
                .checked(|| presence::definitely_empty(&staged.tx, fresh, limits))?;
            encode_public_claim_funding_reply(reason, no_demand, admission, maximum)
                .map_err(codec)?
        }
        funded => {
            if let FundingResult::Paid(paid) = funded {
                staged.tx.checked(|| {
                    settlement::release(
                        &staged.tx,
                        fresh,
                        paid,
                        staged.reply.used_indices(),
                        limits,
                        work,
                    )
                })?;
            }
            let claim = match rejection {
                Some(reason) => staged.reply.finish_rejected(reason),
                None => staged.reply.finish_accepted(),
            }
            .map_err(codec)?;
            let response =
                encode_public_claim_reply(&claim, time, admission, maximum).map_err(codec)?;
            return receipt::finish(
                staged.tx,
                staged.prepared,
                staged.expected,
                staged.control,
                response,
                limits,
            );
        }
    };
    receipt::finish(
        staged.tx,
        staged.prepared,
        staged.expected,
        staged.control,
        response,
        limits,
    )
}
