//! Explicit public parent recovery selection; no unsupported profile is guessed.
use super::{CommittedRecoveryError as E, CommittedRecoveryLimits as Limits};
use wf_domain::workflow::{
    control::{ControlState, EntryStamp},
    request::RequestScope,
};
use wf_ports::log::{
    CodecError, RaftLogEntry, prepare_public_claim_request, public_claim_recovery_header,
};
use wf_storage::{
    state::RedbStateStore,
    transaction::{
        ClaimTransactionOutcome, PublicClaimTransactionLimits, apply_public_claim_transaction,
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::CommittedTime,
};
impl Limits {
    /// Selects public Claim recovery after the actual atomic/GC5 dependencies.
    /// # Errors
    /// Refuses invalid whole credit work/temporary ownership or dependencies.
    pub fn with_public_claim_recovery(mut self, work: u64, temporary: usize) -> Result<Self, E> {
        if self.atomic_claim.is_none() || work == 0 || temporary == 0 {
            return Err(E::InvalidLimits);
        }
        super::workflow_release::require_dependencies(&self)?;
        self.public_claim = Some((work, temporary));
        driver(&self)?;
        self.additional_profile_bytes()?;
        Ok(self)
    }
}

fn driver(limits: &Limits) -> Result<PublicClaimTransactionLimits, E> {
    let base = super::atomic_claim::driver(limits)?;
    let (maximum_credit_work, maximum_credit_temporary_bytes) =
        limits.public_claim.ok_or(E::UnsupportedEntry)?;
    let minimum = base
        .credit
        .max_owned_bytes()
        .checked_mul(2)
        .and_then(|n| n.checked_add(1_048_576))
        .ok_or(E::InvalidLimits)?;
    let native = base.values.record_batch;
    let reserved = super::total(&[
        native.maximum_read_bytes,
        native.maximum_staging_bytes,
        native.maximum_adapter_bytes,
        base.values.maximum_value_temporary_bytes,
        maximum_credit_temporary_bytes,
    ])?;
    if maximum_credit_work == 0
        || maximum_credit_temporary_bytes < minimum
        || reserved >= native.workspace_bytes
    {
        return Err(E::InvalidLimits);
    }
    Ok(PublicClaimTransactionLimits {
        base,
        maximum_credit_work,
        maximum_credit_temporary_bytes,
    })
}

pub(super) fn header<'a>(
    body: &'a [u8],
    limits: &Limits,
) -> Result<(u16, [&'a [u8]; 3], CommittedTime), CodecError> {
    if limits.public_claim.is_none()
        || limits.atomic_claim.is_none()
        || !super::workflow_release::paid(limits)
    {
        return Err(CodecError::UnsupportedTag);
    }
    public_claim_recovery_header(body, limits.normal.codec)
}

pub(super) fn audit(
    body: &[u8],
    limits: &Limits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, E> {
    let driver = driver(limits)?;
    let entry = super::total(&[512, body.len()])?;
    limits.admit(&[base, entry])?;
    let work = super::total(&[65_536, 4096, body.len().checked_mul(16).ok_or(E::Limit)?])?;
    if work > driver.base.values.maximum_value_work {
        return Err(E::Limit);
    }
    let reserved = super::total(&[
        limits.normal.maximum_read_bytes,
        limits.normal.maximum_staging_bytes,
        limits.normal.maximum_adapter_bytes,
        driver.base.values.maximum_value_temporary_bytes,
        driver.maximum_credit_temporary_bytes,
    ])?;
    let native = limits
        .normal
        .workspace_bytes
        .checked_sub(reserved)
        .ok_or(E::Limit)?;
    let available = limits
        .workspace
        .checked_sub(super::total(&[
            base,
            entry,
            limits.additional_profile_bytes()?,
        ])?)
        .ok_or(E::Limit)?
        .min(native);
    drop(
        prepare_public_claim_request(body, limits.normal.codec, driver.base.caller, available)
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
    let original = super::total(&[512, entry.body().len()])?;
    let scope_owner = super::total(&[512, fields[0].len(), fields[1].len(), fields[2].len()])?;
    let conversion = super::total(&[
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
        TenantId::new(super::replay::copy(fields[0])?).map_err(|_| E::NamespaceMismatch)?,
        GroupId::new(super::replay::copy(fields[1])?).map_err(|_| E::NamespaceMismatch)?,
        RecoveryId::new(super::replay::copy(fields[2])?).map_err(|_| E::NamespaceMismatch)?,
    );
    let outcome = apply_public_claim_transaction(state, &scope, entry.body(), entry.id(), driver)
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
