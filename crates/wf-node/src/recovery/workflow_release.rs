//! Explicit GC4/5 recovery with original frames and actual consumed writers.
use super::{CommittedRecoveryError as E, CommittedRecoveryLimits as Limits, replay, total};
use super::{
    catalog_membership::GovernanceCatalogRecoveryProfile as G, credit::CreditRecoveryProfile as C,
};
use wf_domain::{
    governance::credit::CreditLimits,
    workflow::{
        control::{ControlState, EntryStamp},
        request::RequestScope,
    },
};
use wf_ports::log::{
    CodecError, RaftLogEntry, governed_claim_recovery_header, prepare_governed_claim_request,
    prepare_release_maintenance_request, release_maintenance_recovery_header,
};
use wf_storage::{
    state::RedbStateStore,
    transaction::{
        ClaimTransactionOutcome, RecordBatchTransactionOutcome,
        apply_governed_claim_transaction_with_credit_limits, apply_release_maintenance_transaction,
    },
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::CommittedTime,
};

impl Limits {
    /// Selects actual GC4 release-aware mutation and maintenance recovery.
    /// # Errors
    /// Requires stored-value limits and explicit catalog-format2 credit limits.
    pub fn with_workflow_release_recovery(mut self) -> Result<Self, E> {
        self.governance_catalog = G::WorkflowRelease;
        require_dependencies(&self)?;
        Ok(self)
    }
    /// Selects actual GC5 paid-Claim and release recovery.
    /// # Errors
    /// Also requires explicit value-Claim work and publication limits.
    pub fn with_governed_claim_recovery(mut self) -> Result<Self, E> {
        self.governance_catalog = G::GovernedClaim;
        require_dependencies(&self)?;
        Ok(self)
    }
}

pub(super) const fn enabled(limits: &Limits) -> bool {
    matches!(
        limits.governance_catalog,
        G::WorkflowRelease | G::GovernedClaim
    )
}
pub(super) const fn paid(limits: &Limits) -> bool {
    matches!(limits.governance_catalog, G::GovernedClaim)
}
pub(super) fn require_dependencies(limits: &Limits) -> Result<(), E> {
    if !enabled(limits)
        || limits.workflow_stored_value_limits().is_none()
        || !matches!(limits.credit, Some(C::Catalog(..) | C::Public(..)))
        || (paid(limits) && limits.workflow_value_claim_limits().is_none())
        || (limits.atomic_claim.is_some() && !paid(limits))
    {
        return Err(E::InvalidLimits);
    }
    Ok(())
}
fn credit(limits: &Limits) -> Result<CreditLimits, E> {
    require_dependencies(limits)?;
    limits
        .identified_credit_limits()
        .map(|(_, credit)| credit)
        .ok_or(E::InvalidLimits)
}

pub(super) fn header<'a>(
    body: &'a [u8],
    limits: &Limits,
) -> Result<(u16, [&'a [u8]; 3], CommittedTime), CodecError> {
    match body.get(6) {
        Some(16) if enabled(limits) => {
            let (_, fields, stamp) =
                release_maintenance_recovery_header(body, limits.normal.codec)?;
            Ok((18, fields, stamp))
        }
        Some(17) if paid(limits) => governed_claim_recovery_header(body, limits.normal.codec),
        _ => Err(CodecError::UnsupportedTag),
    }
}

pub(super) fn audit(
    body: &[u8],
    limits: &Limits,
    base: usize,
    stamp: CommittedTime,
    profile: u16,
) -> Result<EntryStamp, E> {
    require_dependencies(limits)?;
    let entry = total(&[512, body.len()])?;
    limits.admit(&[base, entry])?;
    let reserved = total(&[
        limits.normal.maximum_read_bytes,
        limits.normal.maximum_staging_bytes,
        limits.normal.maximum_adapter_bytes,
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
    match profile {
        18 => drop(
            prepare_release_maintenance_request(body, limits.normal.codec, available)
                .map_err(E::Codec)?,
        ),
        19 if paid(limits) => {
            let (_, values) = limits
                .workflow_value_claim_limits()
                .ok_or(E::InvalidLimits)?;
            let work = total(&[65_536, 4096, body.len().checked_mul(16).ok_or(E::Limit)?])?;
            if work > values.maximum_value_work {
                return Err(E::Limit);
            }
            let available = available
                .checked_sub(values.maximum_value_temporary_bytes)
                .ok_or(E::Limit)?;
            drop(
                prepare_governed_claim_request(body, limits.normal.codec, available)
                    .map_err(E::Codec)?,
            );
        }
        _ => return Err(E::UnsupportedEntry),
    }
    Ok(EntryStamp::Stamped(stamp))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    fields: [&[u8]; 3],
    limits: &Limits,
    base: usize,
    profile: u16,
) -> Result<ControlState, E> {
    let credit = credit(limits)?;
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
    if profile == 19 && paid(limits) {
        let (work, values) = limits
            .workflow_value_claim_limits()
            .ok_or(E::InvalidLimits)?;
        return apply_governed_claim_transaction_with_credit_limits(
            state,
            &scope,
            entry.body(),
            entry.id(),
            values,
            work,
            credit,
        )
        .map(claim_control)
        .map_err(E::Claim);
    }
    if profile != 18 {
        return Err(E::UnsupportedEntry);
    }
    apply_release_maintenance_transaction(state, &scope, entry.body(), entry.id(), limits.normal)
        .map(normal_control)
        .map_err(E::Normal)
}
fn normal_control(outcome: RecordBatchTransactionOutcome) -> ControlState {
    match outcome {
        RecordBatchTransactionOutcome::Retained { control, outcome } => {
            drop(outcome);
            control
        }
        RecordBatchTransactionOutcome::IntentConflict(control)
        | RecordBatchTransactionOutcome::Expired(control)
        | RecordBatchTransactionOutcome::LogReplay(control) => control,
    }
}
fn claim_control(outcome: ClaimTransactionOutcome) -> ControlState {
    match outcome {
        ClaimTransactionOutcome::Retained { control, outcome } => {
            drop(outcome);
            control
        }
        ClaimTransactionOutcome::IntentConflict(control)
        | ClaimTransactionOutcome::Expired(control)
        | ClaimTransactionOutcome::LogReplay(control) => control,
    }
}
