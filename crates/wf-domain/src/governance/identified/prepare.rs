//! Ordered configuration, expiry and dispatch before any sparse read.

use super::{
    CreditConfiguration, IdentifiedCreditDecision as D, IdentifiedCreditMutation as Op,
    IdentifiedCreditRejection as R, IdentifiedCreditRequest, admission, cleanup,
    protocol::{CreditPhase, PreparedIdentifiedCredit},
};
use crate::governance::credit::{
    CreditDecision as C, CreditError as E, CreditLimits, CreditOperation, CreditOwner,
    CreditOwnerData, admission::CreditCharge, decision,
};
use wf_types::version::TimestampMillis;

/// Prepares one mathematical identified mutation without I/O or publication.
/// # Errors
/// Refuses invalid authority/input, continuous limits or allocation failure.
pub fn prepare_identified_credit(
    owner: Option<CreditOwner>,
    request: IdentifiedCreditRequest<'_>,
    now: TimestampMillis,
    limits: CreditLimits,
) -> Result<PreparedIdentifiedCredit<'_>, E> {
    let data = owner.map(CreditOwner::into_data);
    let mut charge = if let Some(data) = &data {
        CreditCharge::inspect(data, limits)?
    } else {
        CreditCharge::empty(limits)?
    };
    admission::request(request, now.get(), &mut charge)?;
    if let Some(data) = &data {
        admission::detached(data, request, &mut charge)?;
    }
    let mut prepared = PreparedIdentifiedCredit {
        data,
        request,
        now,
        limits,
        charge,
        phase: CreditPhase::Complete,
        decision: D::Rejected(R::NotFound),
        publish_owner: false,
        reservations: Vec::new(),
        pages: Vec::new(),
        cleanup: Vec::new(),
    };
    if prepared.data.is_none() {
        if !matches!(request.mutation, Op::Lease { .. }) {
            return Ok(prepared);
        }
        let Some(config) = request.configuration else {
            abort(&mut prepared, R::ConfigurationRequired);
            return Ok(prepared);
        };
        prepared.data = Some(new_owner(request.scope, config, &mut prepared.charge)?);
    }
    prepared.publish_owner = true;
    if !configure(&mut prepared)? {
        return Ok(prepared);
    }
    if !matches!(request.mutation, Op::Release { .. }) && !cleanup::expire(&mut prepared)? {
        return Ok(prepared);
    }
    dispatch(&mut prepared)?;
    Ok(prepared)
}

fn new_owner(
    scope: &[u8],
    config: CreditConfiguration,
    charge: &mut CreditCharge,
) -> Result<CreditOwnerData, E> {
    charge.own(scope.len())?;
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(scope.len())
        .map_err(|_| E::Allocation)?;
    copied.extend_from_slice(scope);
    Ok(CreditOwnerData {
        scope: copied,
        limit: config.limit,
        free: config.limit,
        epoch: 0,
        config_version: config.config_version.unwrap_or(0),
        policy_fingerprint: config.policy_fingerprint,
        cleanup_head: 1,
        cleanup_tail: 0,
        leases: Vec::new(),
    })
}

fn configure(prepared: &mut PreparedIdentifiedCredit<'_>) -> Result<bool, E> {
    let Some(config) = prepared.request.configuration else {
        return Ok(true);
    };
    let data = prepared.data.as_mut().ok_or(E::InvalidState)?;
    let Some(version) = config.config_version else {
        if config.limit != data.limit {
            abort(prepared, R::ConfigurationVersionRequired);
            return Ok(false);
        }
        return Ok(true);
    };
    if let C::Denied(reason) = decision::apply(
        data,
        CreditOperation::Configure {
            limit: config.limit,
            config_version: version,
            policy_fingerprint: config.policy_fingerprint,
        },
        &mut prepared.charge,
    )? {
        abort(prepared, R::Credit(reason));
        return Ok(false);
    }
    Ok(true)
}

fn dispatch(prepared: &mut PreparedIdentifiedCredit<'_>) -> Result<(), E> {
    match prepared.request.mutation {
        Op::Lease {
            shard_id,
            amount,
            ttl_ms,
        } => {
            let result = decision::apply(
                prepared.data.as_mut().ok_or(E::InvalidState)?,
                CreditOperation::Grant {
                    shard_id,
                    amount,
                    now: prepared.now,
                    ttl_ms,
                },
                &mut prepared.charge,
            )?;
            classify(prepared, result, D::Granted { shard_id });
        }
        Op::Spend {
            shard_id,
            lease_epoch,
            amount,
            ..
        } => {
            if !cleanup::retire(prepared, shard_id)? {
                return Ok(());
            }
            let index = locate(prepared, shard_id)?;
            let data = prepared.data.as_ref().ok_or(E::InvalidState)?;
            let Some(index) = index else {
                prepared.decision = D::Rejected(R::Credit(
                    crate::governance::credit::CreditRejection::LimitExceeded {
                        requested: amount,
                        free: data.free,
                        limit: data.limit,
                    },
                ));
                return Ok(());
            };
            let lease = &data.leases[index];
            if lease.epoch != lease_epoch {
                prepared.decision = D::Rejected(R::LeaseGenerationChanged);
                return Ok(());
            }
            prepared.phase = CreditPhase::Reservations {
                shard_id,
                epoch: lease.epoch,
            };
        }
        Op::Release { shard_id, .. } => {
            prepared.decision = D::Released;
            if let Some(index) = locate(prepared, shard_id)? {
                let lease = &prepared.data.as_ref().ok_or(E::InvalidState)?.leases[index];
                prepared.phase = CreditPhase::Reservations {
                    shard_id,
                    epoch: lease.epoch,
                };
            }
        }
        Op::Renew { shard_id, ttl_ms } => {
            let result = decision::apply(
                prepared.data.as_mut().ok_or(E::InvalidState)?,
                CreditOperation::Renew {
                    shard_id,
                    now: prepared.now,
                    ttl_ms,
                },
                &mut prepared.charge,
            )?;
            classify(prepared, result, D::Renewed { shard_id });
        }
        Op::Get => prepared.decision = D::Owner,
        Op::Cleanup => cleanup::start(prepared)?,
    }
    Ok(())
}

pub(super) fn classify(prepared: &mut PreparedIdentifiedCredit<'_>, result: C, accepted: D) {
    prepared.phase = CreditPhase::Complete;
    prepared.decision = match result {
        C::Denied(reason) => D::Rejected(R::Credit(reason)),
        _ => accepted,
    };
}

pub(super) fn abort(prepared: &mut PreparedIdentifiedCredit<'_>, reason: R) {
    prepared.data = None;
    prepared.publish_owner = false;
    prepared.reservations.clear();
    prepared.pages.clear();
    prepared.cleanup.clear();
    prepared.phase = CreditPhase::Complete;
    prepared.decision = D::Rejected(reason);
}

pub(super) fn locate(
    prepared: &mut PreparedIdentifiedCredit<'_>,
    shard: u64,
) -> Result<Option<usize>, E> {
    let data = prepared.data.as_ref().ok_or(E::InvalidState)?;
    prepared
        .charge
        .work(crate::governance::credit::admission::product(
            data.leases.len(),
            16,
        )?)?;
    Ok(data
        .leases
        .binary_search_by_key(&shard, |lease| lease.shard_id)
        .ok())
}
