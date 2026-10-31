//! Explicit original atomic-parent recovery; generic profiles stay closed.
use super::{CommittedRecoveryError as E, CommittedRecoveryLimits as Limits, replay, total};
use wf_domain::{
    claim::{caller::ClaimCallerLimits, coordination::ClaimCoordinationLimits},
    workflow::{
        control::{ControlState, EntryStamp},
        request::RequestScope,
    },
};
use wf_ports::log::{
    CodecError, RaftLogEntry, atomic_claim_recovery_header, prepare_atomic_claim_request,
};
use wf_storage::{
    state::RedbStateStore,
    transaction::{
        AtomicClaimTransactionLimits, ClaimTransactionOutcome, apply_atomic_claim_transaction,
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::CommittedTime,
};

impl Limits {
    /// Selects original atomic Claim recovery with explicit caller/schedule limits.
    /// # Errors
    /// Requires the actual GC5, stored-value, catalog-credit and value-Claim profile.
    pub fn with_atomic_claim_recovery(
        mut self,
        caller: ClaimCallerLimits,
        coordination: ClaimCoordinationLimits,
    ) -> Result<Self, E> {
        self.atomic_claim = Some((caller, coordination));
        driver(&self)?;
        self.additional_profile_bytes()?;
        Ok(self)
    }
}

pub(super) fn driver(limits: &Limits) -> Result<AtomicClaimTransactionLimits, E> {
    super::workflow_release::require_dependencies(limits)?;
    if !super::workflow_release::paid(limits) {
        return Err(E::InvalidLimits);
    }
    let (caller, coordination) = limits.atomic_claim.ok_or(E::UnsupportedEntry)?;
    let (work, values) = limits
        .workflow_value_claim_limits()
        .ok_or(E::InvalidLimits)?;
    let (_, credit) = limits.identified_credit_limits().ok_or(E::InvalidLimits)?;
    Ok(AtomicClaimTransactionLimits {
        values,
        work,
        caller,
        coordination,
        credit,
    })
}

pub(super) fn header<'a>(
    body: &'a [u8],
    limits: &Limits,
) -> Result<(u16, [&'a [u8]; 3], CommittedTime), CodecError> {
    if limits.atomic_claim.is_none() || !super::workflow_release::paid(limits) {
        return Err(CodecError::UnsupportedTag);
    }
    atomic_claim_recovery_header(body, limits.normal.codec)
}

pub(super) fn audit(
    body: &[u8],
    limits: &Limits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, E> {
    let driver = driver(limits)?;
    let entry = total(&[512, body.len()])?;
    limits.admit(&[base, entry])?;
    let work = total(&[65_536, 4096, body.len().checked_mul(16).ok_or(E::Limit)?])?;
    if work > driver.values.maximum_value_work {
        return Err(E::Limit);
    }
    let reserved = total(&[
        limits.normal.maximum_read_bytes,
        limits.normal.maximum_staging_bytes,
        limits.normal.maximum_adapter_bytes,
        driver.values.maximum_value_temporary_bytes,
    ])?;
    let native = limits
        .normal
        .workspace_bytes
        .checked_sub(reserved)
        .ok_or(E::Limit)?;
    let available = limits
        .workspace
        .checked_sub(total(&[base, entry, limits.additional_profile_bytes()?])?)
        .ok_or(E::Limit)?
        .min(native);
    drop(
        prepare_atomic_claim_request(body, limits.normal.codec, driver.caller, available)
            .map_err(E::Codec)?,
    );
    Ok(EntryStamp::Stamped(stamp))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    fields: [&[u8]; 3],
    limits: &Limits,
    base: usize,
) -> Result<ControlState, E> {
    let driver = driver(limits)?;
    let original = total(&[512, entry.body().len()])?;
    let scope_owner = total(&[512, fields[0].len(), fields[1].len(), fields[2].len()])?;
    let conversion = total(&[
        size_of::<Vec<u8>>(),
        fields[0].len().max(fields[1].len()).max(fields[2].len()),
    ])?;
    limits.admit(&[
        base,
        original,
        scope_owner,
        limits.normal.workspace_bytes.max(conversion),
    ])?;
    let scope = RequestScope::new(
        TenantId::new(replay::copy(fields[0])?).map_err(|_| E::NamespaceMismatch)?,
        GroupId::new(replay::copy(fields[1])?).map_err(|_| E::NamespaceMismatch)?,
        RecoveryId::new(replay::copy(fields[2])?).map_err(|_| E::NamespaceMismatch)?,
    );
    let outcome = apply_atomic_claim_transaction(state, &scope, entry.body(), entry.id(), driver)
        .map_err(E::Claim)?;
    Ok(match outcome {
        ClaimTransactionOutcome::Retained { control, outcome } => {
            drop(outcome);
            control
        }
        ClaimTransactionOutcome::IntentConflict(control)
        | ClaimTransactionOutcome::Expired(control)
        | ClaimTransactionOutcome::LogReplay(control) => control,
    })
}
