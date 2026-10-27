//! Coherent scalar capture, bounded points and the complete applied-prefix audit.

use super::{CommittedRecoveryError as Error, CommittedRecoveryLimits, total};
use sha2::{Digest, Sha256};
use wf_consensus::apply::read_applied_state;
use wf_domain::policy::complete::{CompleteCorePolicyPatchApplyError, PolicySchemaCharge};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_ports::log::{
    CodecError, RaftLogEntry, RaftLogStore, measure_control_entry, normal_recovery_header,
    prepare_atomic_start, prepare_claim_phase, prepare_independent_record_batch,
    prepare_public_mutation, prepare_record_batch, stored_value_recovery_header,
    verify_complete_core_policy_patch_frame,
};
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_storage::log::RedbLogStore;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    ClaimTransactionError, CorePolicyTransactionError, RecordBatchTransactionError,
    WorkflowValueTransactionError,
};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::{CommittedTime, LogIdentity};

pub(super) fn observe(
    state: &mut RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    base: usize,
    membership: &mut Option<(EntryIdentity, EntryStamp)>,
) -> Result<ControlState, Error> {
    if limits.stored_workflow_values {
        limits.admit(&[
            base,
            limits.state.1,
            limits.state.3,
            std::mem::size_of::<bool>(),
        ])?;
    } else {
        limits.admit(&[base, limits.state.1, limits.state.3])?;
    }
    let admission = AdmissionLimits::new(1, 1, 1).map_err(|_| Error::InvalidLimits)?;
    let tx = state
        .begin_apply(
            namespace.0,
            namespace.1,
            admission,
            limits.state.0,
            limits.state.1,
            limits.state.3,
        )
        .map_err(Error::State)?;
    if limits.stored_workflow_values {
        tx.require_stored_workflow_values()
            .map_err(|error| match error {
                StateTransactionError::UnsupportedLayout
                | StateTransactionError::UnsupportedPolicyProfile => {
                    Error::Value(WorkflowValueTransactionError::UnsupportedProfile)
                }
                other => Error::State(other),
            })?;
    }
    let (control, value) =
        read_applied_state(&tx, limits.membership, limits.workspace).map_err(Error::State)?;
    *membership = match (value.entry(), value.stamp()) {
        (None, None) => None,
        (Some(entry), Some(stamp)) => Some((entry, stamp)),
        _ => return Err(Error::ProgressMismatch),
    };
    // Only Copy metadata survives this phase; no membership U meets a log point.
    drop(value);
    drop(tx);
    Ok(control)
}

pub(super) fn boundary(
    applied: ControlState,
    committed: Option<LogIdentity>,
    limits: &CommittedRecoveryLimits,
) -> Result<u128, Error> {
    if let Some(entry) = applied.last_entry() {
        let end = committed.ok_or(Error::IncompatibleCheckpoint)?;
        if entry.log().index() > end.index()
            || (entry.log().index() == end.index() && entry.log() != end)
        {
            return Err(Error::IncompatibleCheckpoint);
        }
    }
    let stop = committed.map_or(0, |id| u128::from(id.index().get()) + 1);
    if stop > limits.work.0 as u128 {
        return Err(Error::Limit);
    }
    Ok(stop)
}

pub(super) fn point(
    log: &mut RedbLogStore,
    index: u64,
    limits: &CommittedRecoveryLimits,
    base: usize,
    work: &mut [usize; 2],
) -> Result<RaftLogEntry, Error> {
    work[0] = total(&[work[0], 1])?;
    if work[0] > limits.work.0 {
        return Err(Error::Limit);
    }
    let cap = 512 + (limits.entry - 512).min(work[1]);
    limits.admit(&[base, cap])?;
    let entry = log
        .read_entry(index, cap)
        .map_err(Error::Log)?
        .ok_or(Error::MissingEntry)?;
    work[1] = work[1]
        .checked_sub(entry.body().len())
        .ok_or(Error::Limit)?;
    if entry.id().index().get() != index {
        return Err(Error::IdentityMismatch);
    }
    Ok(entry)
}

pub(super) fn identity(entry: &RaftLogEntry) -> EntryIdentity {
    EntryIdentity::new(
        entry.id(),
        PreparedDigest::new(Sha256::digest(entry.body()).into()),
    )
}

pub(super) fn normal_header<'a>(
    body: &'a [u8],
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
) -> Result<(u16, [&'a [u8]; 3], CommittedTime), Error> {
    let header = if limits.stored_workflow_values {
        stored_value_recovery_header(body, limits.normal.codec)
    } else {
        normal_recovery_header(body, limits.normal.codec)
    }
    .map_err(|error| {
        if error == CodecError::UnsupportedTag {
            Error::UnsupportedEntry
        } else {
            Error::Codec(error)
        }
    })?;
    if header.1[1] != namespace.0.as_bytes() || header.1[2] != namespace.1.as_bytes() {
        return Err(Error::NamespaceMismatch);
    }
    Ok(header)
}

pub(super) fn control(
    body: &[u8],
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
) -> Result<(EntryStamp, bool), Error> {
    let (stamp, owner) = measure_control_entry(body, namespace.0, namespace.1, limits.membership)
        .map_err(Error::Membership)?;
    Ok((stamp, owner.is_some()))
}

pub(super) fn audit(
    entry: &RaftLogEntry,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    base: usize,
    applied: ControlState,
    latest: &mut Option<(EntryIdentity, EntryStamp)>,
) -> Result<(), Error> {
    let identity = identity(entry);
    let at_applied = applied.position() == entry.id().index();
    if at_applied {
        let saved = applied.last_entry().ok_or(Error::IncompatibleCheckpoint)?;
        if saved.log() != identity.log() {
            return Err(Error::IdentityMismatch);
        }
        if saved.prepared() != identity.prepared() {
            return Err(Error::BodyMismatch);
        }
    }
    let (stamp, member) = match entry.body().get(..4) {
        Some(b"FWCP") => (audit_normal(entry.body(), namespace, limits, base)?, false),
        Some(b"FWBL" | b"FWMB") => control(entry.body(), namespace, limits)?,
        _ => return Err(Error::UnsupportedEntry),
    };
    if at_applied && applied.last_entry_stamp() != Some(stamp) {
        return Err(Error::StampMismatch);
    }
    if let EntryStamp::Stamped(time) = stamp
        && (applied.time().hlc() < time.hlc() || applied.time().wall() < time.wall())
    {
        return Err(Error::ClockMismatch);
    }
    if member {
        *latest = Some((identity, stamp));
    }
    Ok(())
}

fn audit_normal(
    body: &[u8],
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    base: usize,
) -> Result<EntryStamp, Error> {
    let (profile, _, stamp) = normal_header(body, namespace, limits)?;
    if profile == 2 {
        return audit_core_policy(body, limits, base, stamp);
    }
    if profile == 4 {
        return super::original_policy::audit(body, limits, base, stamp);
    }
    if matches!(profile, 8 | 9) {
        return super::workflow_values::audit(body, limits, base, stamp, profile);
    }
    let available = limits
        .workspace
        .checked_sub(total(&[base, 512])?)
        .ok_or(Error::Limit)?;
    let codec = limits.normal.codec;
    let decoded = if profile == 3 {
        limits.claim_work.ok_or(Error::UnsupportedEntry)?;
        prepare_claim_phase(body, codec, available)
            .map_err(|error| Error::Claim(ClaimTransactionError::Apply(error)))?
    } else {
        match profile {
            5 => prepare_record_batch(body, codec, available),
            6 => prepare_independent_record_batch(body, codec, available),
            7 => prepare_atomic_start(body, codec, available),
            1 => prepare_public_mutation(body, codec, available),
            _ => return Err(Error::UnsupportedEntry),
        }
        .map_err(|error| Error::Normal(RecordBatchTransactionError::Apply(error)))?
    };
    if decoded.request().scope.group() != namespace.0
        || decoded.request().scope.recovery() != namespace.1
    {
        return Err(Error::NamespaceMismatch);
    }
    let stamp = EntryStamp::Stamped(decoded.request().stamp);
    drop(decoded);
    Ok(stamp)
}

fn audit_core_policy(
    body: &[u8],
    limits: &CommittedRecoveryLimits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, Error> {
    if !limits.core_policy {
        return Err(Error::UnsupportedEntry);
    }
    let header = body
        .len()
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(2048))
        .ok_or(Error::Limit)?;
    limits.admit(&[base, 512, header])?;
    let driver = limits.normal;
    let mut charge = PolicySchemaCharge::new(driver.schema);
    let verified = verify_complete_core_policy_patch_frame(
        body,
        driver.schema,
        driver.codec.admission(),
        &mut charge,
    )
    .map_err(|error| {
        Error::CorePolicy(CorePolicyTransactionError::Apply(
            CompleteCorePolicyPatchApplyError::Request(error),
        ))
    })?;
    // The verifier uses the original body/grammar and owns no reconstructed Patch.
    drop(verified);
    Ok(EntryStamp::Stamped(stamp))
}
