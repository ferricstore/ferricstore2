//! Complete current governance and exact original payment under the real writer.
use super::{ClaimRejection, E, PreparedAtomicClaim, RedbStateTransaction, TimestampMillis};
use crate::{
    state::redb::{
        credit::CreditReadContext,
        history_snapshot, publication,
        release::claim::{VerifiedGovernedClaimWrites, available_at},
    },
    transaction::AtomicClaimTransactionLimits,
};
use wf_domain::{
    claim::coordination::{
        CurrentRunningCreditError, RunningCreditDemand, RunningCreditDemandKind,
        resolve_current_running_credit,
    },
    governance::{caller::CreditPolicyInput, credit::CreditLimits},
    policy::complete::{PolicySchemaCharge, RunningClaimPolicyVersion},
    retry::source_binary_hash,
    workflow::{indexed::IndexedBatchPlan, request::RequestScope},
};

use wf_ports::{
    log::{fingerprint_credit_policy, running_claim_scope},
    state::GroupApplyTransaction,
};

pub(super) fn initial(
    tx: &RedbStateTransaction<'_>,
    prepared: &PreparedAtomicClaim<'_>,
    time: TimestampMillis,
    limits: AtomicClaimTransactionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<ClaimRejection>, E> {
    let call = prepared.caller(limits.caller).map_err(super::codec)?;
    let shards = tx.require_identified_credit()?;
    let schema = limits.values.record_batch.schema;
    // Admit the original caller/time/pair before any unused policy observation.
    let explicit = resolve_current_running_credit(
        &call,
        None,
        time,
        shards,
        schema,
        charge,
        limits.coordination,
    )
    .map_err(current_error)?;
    let stored = if explicit.is_none() {
        tx.stored_policy_value(
            prepared.scope().tenant(),
            call.workflow_type(),
            schema,
            charge,
        )?
    } else {
        None
    };
    let demand = match explicit {
        Some(demand) => Some(demand),
        None => resolve_current_running_credit(
            &call,
            stored.as_ref(),
            time,
            shards,
            schema,
            charge,
            limits.coordination,
        )
        .map_err(current_error)?,
    };
    let (demand, paid) = match (demand, prepared.payment()) {
        (None, None) => return Ok(None),
        (Some(_), None) => return Ok(Some(ClaimRejection::GoverningReservationUnavailable)),
        (None, Some(_)) => return Err(E::InvalidChanges),
        (Some(demand), Some(paid)) => (demand, paid),
    };
    match demand.kind() {
        RunningCreditDemandKind::Explicit { scope, shard_id } => {
            if paid.scope() != scope || paid.shard_id() != shard_id {
                return Err(E::InvalidChanges);
            }
        }
        RunningCreditDemandKind::PolicyDerived {
            workflow_type,
            worker,
        } => {
            if !(1..=1_u64 << 32).contains(&shards) {
                return Err(E::UnsupportedPolicyProfile);
            }
            if paid.scope() != running_claim_scope(workflow_type)
                || paid.shard_id() != u64::from(source_binary_hash(worker)) % shards
            {
                return Err(E::InvalidChanges);
            }
            if !configured(tx, prepared, &demand, limits.credit)? {
                return Ok(Some(ClaimRejection::GoverningReservationUnavailable));
            }
        }
    }
    if paid.enforcement() != demand.enforcement() {
        return Err(E::InvalidChanges);
    }
    if u64::try_from(paid.count()).map_err(|_| E::Limit)? != demand.amount()
        || !available_at(
            tx,
            prepared.scope(),
            paid,
            time,
            paid.count(),
            0,
            limits.credit,
        )?
    {
        return Ok(Some(ClaimRejection::GoverningReservationUnavailable));
    }
    Ok(None)
}

fn configured(
    tx: &RedbStateTransaction<'_>,
    prepared: &PreparedAtomicClaim<'_>,
    demand: &RunningCreditDemand<'_, '_>,
    credit: CreditLimits,
) -> Result<bool, E> {
    let Some(version) = demand.config_version() else {
        // The source omits configuration when generation/customer version is absent.
        return Ok(true);
    };
    let paid = prepared.payment().ok_or(E::InvalidChanges)?;
    let context = CreditReadContext::new(tx, prepared.scope(), paid.scope(), credit)?;
    let Some(owner) = context.owner()? else {
        return Ok(false);
    };
    let fingerprint = demand
        .policy_version()
        .map(|version| {
            fingerprint_credit_policy(match version {
                RunningClaimPolicyVersion::Number(n) => CreditPolicyInput::Number(n),
                RunningClaimPolicyVersion::Bytes(bytes) => CreditPolicyInput::Bytes(bytes),
            })
        })
        .transpose()
        .map_err(super::codec)?;
    let data = owner.view();
    Ok(data.limit == demand.limit()
        && data.config_version == version
        && data.policy_fingerprint == fingerprint)
}

fn current_error(error: CurrentRunningCreditError) -> E {
    match error {
        CurrentRunningCreditError::Coordination(_) => E::InvalidChanges,
        CurrentRunningCreditError::Policy(error) => {
            super::super::super::original_policy::original_error(error)
        }
    }
}

pub(super) fn write_records(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    business: &IndexedBatchPlan<'_>,
    verified: &VerifiedGovernedClaimWrites<'_>,
) -> Result<(), E> {
    for (item, ordering) in business.batch().items().iter().zip(business.metadata()) {
        let write = item.plan().write().ok_or(E::InvalidChanges)?;
        publication::write_governed_claim_record(
            tx,
            scope,
            item.partition(),
            &write.record,
            ordering,
            verified,
        )?;
        let history = item.plan().history().ok_or(E::InvalidChanges)?;
        history_snapshot::write(
            tx,
            scope,
            item.partition(),
            &write.record,
            ordering,
            history,
        )?;
    }
    Ok(())
}
