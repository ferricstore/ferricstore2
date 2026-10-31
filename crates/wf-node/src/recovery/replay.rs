//! Ordered real-driver dispatch; no body wrapping, second decode or reply queue.

use super::{CommittedRecoveryError as Error, CommittedRecoveryLimits, join, total};
use wf_consensus::apply::apply_control_entry;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryStamp, plan_control};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{CodecError, RaftLogEntry};
use wf_ports::state::GroupApplyStore;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionLimits, ClaimTransactionOutcome, RecordBatchTransactionOutcome,
    apply_atomic_start_transaction, apply_claim_transaction,
    apply_complete_core_policy_patch_transaction, apply_independent_record_batch_transaction,
    apply_public_mutation_transaction, apply_record_batch_transaction,
};
use wf_types::identity::{GroupId, RecoveryId, TenantId};

pub(super) fn apply(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    base: usize,
    previous: ControlState,
) -> Result<ControlState, Error> {
    let stamp = match entry.body().get(..4) {
        Some(b"FWCP") => {
            EntryStamp::Stamped(join::normal_header(entry.body(), namespace, limits)?.2)
        }
        Some(b"FWBL" | b"FWMB") => join::control(entry.body(), namespace, limits)?.0,
        _ => return Err(Error::UnsupportedEntry),
    };
    let expected = plan_control(previous, join::identity(entry), stamp)
        .map_err(|_| Error::ProgressMismatch)?
        .next();
    let observed = if entry.body().starts_with(b"FWCP") {
        normal(state, entry, namespace, limits, base)?
    } else {
        control(state, entry, namespace, limits, base)?
    };
    if observed != expected {
        return Err(Error::ProgressMismatch);
    }
    Ok(observed)
}

fn control(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    base: usize,
) -> Result<ControlState, Error> {
    let actual = total(&[512, entry.body().len()])?;
    limits.admit(&[base, actual, limits.state.2, limits.state.3])?;
    let admission = AdmissionLimits::new(1, 1, 1).map_err(|_| Error::InvalidLimits)?;
    let tx = state
        .begin_apply(
            namespace.0,
            namespace.1,
            admission,
            limits.state.0,
            limits.state.2,
            limits.state.3,
        )
        .map_err(Error::State)?;
    apply_control_entry(
        tx,
        namespace.0,
        namespace.1,
        entry.body(),
        entry.id(),
        limits.membership,
        limits.workspace,
    )
    .map_err(Error::State)
}

fn normal(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    base: usize,
) -> Result<ControlState, Error> {
    let (profile, fields, _) = join::normal_header(entry.body(), namespace, limits)?;
    if profile == 20 {
        return super::atomic_claim::apply(state, entry, fields, limits, base);
    }
    if matches!(profile, 18 | 19) {
        return super::workflow_release::apply(state, entry, fields, limits, base, profile);
    }
    if matches!(profile, 12..=17) {
        return governance(state, entry, fields, limits, base, profile);
    }
    if matches!(profile, 8..=11) || (profile == 3 && limits.value_claim_work.is_some()) {
        return super::workflow_values::apply(state, entry, fields, (limits, base, profile));
    }
    if profile == 2 && !limits.core_policy {
        return Err(Error::UnsupportedEntry);
    }
    if profile == 4 && !limits.original_policy {
        return Err(Error::UnsupportedEntry);
    }
    let claim_work = if profile == 3 {
        Some(limits.claim_work.ok_or(Error::UnsupportedEntry)?)
    } else {
        None
    };
    let scope_bytes = total(&[512, fields[0].len(), fields[1].len(), fields[2].len()])?;
    let driver = limits.normal;
    let before = total(&[512, entry.body().len(), scope_bytes])?;
    let during = total(&[512, scope_bytes, driver.workspace_bytes])?;
    limits.admit(&[base, before.max(during)])?;
    let scope = RequestScope::new(
        TenantId::new(copy(fields[0])?).map_err(|_| Error::Codec(CodecError::Malformed))?,
        GroupId::new(copy(namespace.0.as_bytes())?)
            .map_err(|_| Error::Codec(CodecError::Malformed))?,
        RecoveryId::new(copy(namespace.1.as_bytes())?)
            .map_err(|_| Error::Codec(CodecError::Malformed))?,
    );
    if let Some(work) = claim_work {
        let outcome = apply_claim_transaction(
            state,
            &scope,
            entry.body(),
            entry.id(),
            ClaimTransactionLimits { base: driver, work },
        )
        .map_err(Error::Claim)?;
        let control = match outcome {
            ClaimTransactionOutcome::Retained { control, outcome } => {
                drop(outcome);
                control
            }
            ClaimTransactionOutcome::IntentConflict(control)
            | ClaimTransactionOutcome::Expired(control)
            | ClaimTransactionOutcome::LogReplay(control) => control,
        };
        drop(scope);
        return Ok(control);
    }
    let outcome = if profile == 4 {
        super::original_policy::apply(state, &scope, entry, driver)?
    } else if profile == 2 {
        apply_complete_core_policy_patch_transaction(
            state,
            &scope,
            entry.body(),
            entry.id(),
            driver,
        )
        .map_err(Error::CorePolicy)?
    } else {
        match profile {
            5 => apply_record_batch_transaction(state, &scope, entry.body(), entry.id(), driver),
            6 => apply_independent_record_batch_transaction(
                state,
                &scope,
                entry.body(),
                entry.id(),
                driver,
            ),
            1 => apply_public_mutation_transaction(state, &scope, entry.body(), entry.id(), driver),
            7 => apply_atomic_start_transaction(state, &scope, entry.body(), entry.id(), driver),
            _ => return Err(Error::UnsupportedEntry),
        }
        .map_err(Error::Normal)?
    };
    let control = match outcome {
        RecordBatchTransactionOutcome::Retained { control, outcome } => {
            drop(outcome);
            control
        }
        RecordBatchTransactionOutcome::IntentConflict(control)
        | RecordBatchTransactionOutcome::Expired(control)
        | RecordBatchTransactionOutcome::LogReplay(control) => control,
    };
    drop(scope);
    Ok(control)
}

pub(super) fn copy(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::State(wf_ports::state::StateTransactionError::Allocation))?;
    output.extend_from_slice(bytes);
    Ok(output)
}

fn governance(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    fields: [&[u8]; 3],
    limits: &CommittedRecoveryLimits,
    base: usize,
    profile: u16,
) -> Result<ControlState, Error> {
    match profile {
        12 | 17 => super::credit::apply(state, entry, fields, limits, base),
        13 => super::catalog_ack::apply(state, entry, fields, limits, base),
        14 => super::catalog_membership::apply(state, entry, fields, limits, base),
        15 => super::catalog_progress::apply(state, entry, fields, limits, base),
        16 => super::catalog_prune::apply(state, entry, fields, limits, base),
        _ => Err(Error::UnsupportedEntry),
    }
}
