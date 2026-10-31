//! Source-ordered real spend, optional lease, then spend in this original writer.
use crate::{
    state::redb::{
        credit::{CreditReadContext, stage_claim_credit},
        store::RedbStateTransaction,
    },
    transaction::PublicClaimTransactionLimits as Limits,
};
use wf_domain::{
    claim::coordination::{
        CurrentRunningCreditError, RunningCreditDemand, RunningCreditDemandKind,
        resolve_current_running_credit,
    },
    governance::{
        caller::CreditPolicyInput,
        identified::{
            CreditConfiguration, IdentifiedCreditDecision as D, IdentifiedCreditMutation as M,
            IdentifiedCreditRejection as R, IdentifiedCreditRequest,
        },
        release::LimitEnforcement,
    },
    policy::complete::{PolicySchemaCharge, RunningClaimPolicyVersion},
    retry::source_binary_hash,
};
use wf_ports::{
    log::{
        CreditSpendPreparation, FreshAtomicClaim, GovernedClaimPayment, encode_claim_payment,
        fingerprint_credit_policy, generate_public_credit_ids, running_claim_scope,
    },
    state::{GroupApplyTransaction, StateTransactionError as E},
};

pub(crate) enum FundingResult {
    Unpaid,
    Paid(FundedClaimPayment),
    Rejected(R),
}
pub(crate) struct FundedClaimPayment {
    pub scope: Vec<u8>,
    pub shard: u64,
    pub shards: u64,
    pub epoch: u64,
    pub ids: Vec<Vec<u8>>,
    pub enforcement: LimitEnforcement,
}
impl FundedClaimPayment {
    pub(super) fn encode(&self, limits: Limits, maximum: usize) -> Result<Vec<u8>, E> {
        encode_claim_payment(
            GovernedClaimPayment {
                scope: &self.scope,
                shard_id: self.shard,
                epoch: self.epoch,
                enforcement: self.enforcement,
                reservation_ids: &self.ids,
            },
            limits.base.values.record_batch.codec,
            maximum,
        )
        .map_err(super::codec)
    }
}

pub(super) fn initialize(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    limits: Limits,
    schema: &mut PolicySchemaCharge,
    work: &mut u64,
) -> Result<FundingResult, E> {
    let root = fresh.prepared();
    let call = root.caller(limits.base.caller).map_err(super::codec)?;
    let shards = tx.require_identified_credit()?;
    let profile = limits.base.values.record_batch.schema;
    let time = fresh.control().execution_time();
    let explicit = resolve_current_running_credit(
        &call,
        None,
        time,
        shards,
        profile,
        schema,
        limits.base.coordination,
    )
    .map_err(current_error)?;
    let stored = if explicit.is_none() {
        tx.stored_policy_value(root.scope().tenant(), call.workflow_type(), profile, schema)?
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
            profile,
            schema,
            limits.base.coordination,
        )
        .map_err(current_error)?,
    };
    let Some(demand) = demand else {
        return Ok(FundingResult::Unpaid);
    };
    fund(tx, fresh, &demand, shards, limits, work)
}

fn fund(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    demand: &RunningCreditDemand<'_, '_>,
    shards: u64,
    limits: Limits,
    work: &mut u64,
) -> Result<FundingResult, E> {
    let derived;
    let (scope, shard) = match demand.kind() {
        RunningCreditDemandKind::Explicit { scope, shard_id } => (scope, shard_id),
        RunningCreditDemandKind::PolicyDerived {
            workflow_type,
            worker,
        } => {
            if !(1..=1_u64 << 32).contains(&shards) {
                return Err(E::UnsupportedPolicyProfile);
            }
            derived = running_claim_scope(workflow_type);
            (
                derived.as_slice(),
                u64::from(source_binary_hash(worker)) % shards,
            )
        }
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
    let configuration = CreditConfiguration {
        limit: demand.limit(),
        config_version: demand.config_version(),
        policy_fingerprint: fingerprint,
    };
    let spend_configuration = demand.config_version().map(|_| configuration);
    let mut paid = FundedClaimPayment {
        scope: copy(scope)?,
        shard,
        shards,
        epoch: 0,
        ids: Vec::new(),
        enforcement: demand.enforcement(),
    };
    let first = spend(
        tx,
        fresh,
        &mut paid,
        demand,
        spend_configuration,
        limits,
        work,
    )?;
    match first {
        D::Spent { .. } => return Ok(FundingResult::Paid(paid)),
        D::Rejected(reason) if !demand.auto_lease() => return Ok(FundingResult::Rejected(reason)),
        D::Rejected(_) => (),
        _ => return Err(E::Corrupt),
    }
    let lease = stage_claim_credit(
        tx,
        fresh,
        IdentifiedCreditRequest {
            scope: &paid.scope,
            shard_count: shards,
            configuration: Some(configuration),
            mutation: M::Lease {
                shard_id: shard,
                amount: demand.amount().max(demand.lease_size()),
                ttl_ms: demand.ttl_ms(),
            },
        },
        limits.base.credit,
        (work, limits.maximum_credit_work),
    )?;
    match lease {
        D::Granted { .. } => (),
        D::Rejected(reason) => return Ok(FundingResult::Rejected(reason)),
        _ => return Err(E::Corrupt),
    }
    match spend(
        tx,
        fresh,
        &mut paid,
        demand,
        spend_configuration,
        limits,
        work,
    )? {
        D::Spent { .. } => Ok(FundingResult::Paid(paid)),
        D::Rejected(reason) => Ok(FundingResult::Rejected(reason)),
        _ => Err(E::Corrupt),
    }
}

fn spend(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    paid: &mut FundedClaimPayment,
    demand: &RunningCreditDemand<'_, '_>,
    configuration: Option<CreditConfiguration>,
    limits: Limits,
    work: &mut u64,
) -> Result<D, E> {
    let context = CreditReadContext::new(
        tx,
        fresh.prepared().scope(),
        &paid.scope,
        limits.base.credit,
    )?;
    let Some(owner) = context.owner()? else {
        return Ok(D::Rejected(R::NotFound));
    };
    // Exact source public-credit sentinel when the actual owner has no shard lease.
    paid.epoch = owner.lease(paid.shard).map_or(0, |lease| lease.epoch);
    drop(owner);
    paid.ids.clear();
    paid.ids = generate_public_credit_ids(
        demand.amount(),
        CreditSpendPreparation {
            lease_epoch: paid.epoch,
            batch: fresh.prepared().funding_batch().ok_or(E::InvalidChanges)?,
        },
        limits.maximum_credit_temporary_bytes,
    )
    .map_err(super::codec)?;
    stage_claim_credit(
        tx,
        fresh,
        IdentifiedCreditRequest {
            scope: &paid.scope,
            shard_count: paid.shards,
            configuration,
            mutation: M::Spend {
                shard_id: paid.shard,
                lease_epoch: paid.epoch,
                amount: demand.amount(),
                ttl_ms: Some(demand.ttl_ms()),
                reservation_ids: &paid.ids,
            },
        },
        limits.base.credit,
        (work, limits.maximum_credit_work),
    )
}
pub(super) fn copy(bytes: &[u8]) -> Result<Vec<u8>, E> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| E::Allocation)?;
    out.extend_from_slice(bytes);
    Ok(out)
}
fn current_error(error: CurrentRunningCreditError) -> E {
    match error {
        CurrentRunningCreditError::Coordination(_) => E::InvalidChanges,
        CurrentRunningCreditError::Policy(error) => {
            crate::state::redb::original_policy::original_error(error)
        }
    }
}
