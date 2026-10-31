//! Complete ACK historical audit and real replay only under Catalog selection.

use super::{
    CommittedRecoveryError as E, CommittedRecoveryLimits, credit::CreditRecoveryProfile, replay,
    total,
};
use wf_domain::{
    governance::credit::CreditLimits,
    workflow::{
        control::{ControlState, EntryStamp},
        request::RequestScope,
    },
};
use wf_ports::log::{RaftLogEntry, prepare_catalog_ack_request};
use wf_storage::{
    state::RedbStateStore,
    transaction::{RecordBatchTransactionOutcome, apply_catalog_ack_transaction},
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::CommittedTime,
};

pub(super) const fn enabled(limits: &CommittedRecoveryLimits) -> bool {
    matches!(
        limits.credit,
        Some(CreditRecoveryProfile::Catalog(..) | CreditRecoveryProfile::Public(..))
    )
}

fn selected(limits: &CommittedRecoveryLimits) -> Result<CreditLimits, E> {
    match limits.credit {
        Some(
            CreditRecoveryProfile::Catalog(_, credit) | CreditRecoveryProfile::Public(_, credit),
        ) => Ok(credit),
        _ => Err(E::UnsupportedEntry),
    }
}

pub(super) fn audit(
    body: &[u8],
    limits: &CommittedRecoveryLimits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, E> {
    let credit = selected(limits)?;
    let entry = total(&[512, body.len()])?;
    limits.admit(&[base, entry])?;
    let available = limits
        .workspace
        .checked_sub(total(&[base, entry, limits.additional_profile_bytes()?])?)
        .ok_or(E::Limit)?
        .min(credit.max_owned_bytes());
    // Fixed256-entry ACK work, independent of the workflow batch item ceiling.
    // This conservative logical fee is not a native CPU/engine I/O measurement.
    let cost = body
        .len()
        .checked_mul(32)
        .and_then(|n| n.checked_add(65_536))
        .and_then(|n| n.checked_add(256 * 65_536))
        .ok_or(E::Limit)?;
    if u64::try_from(cost).map_err(|_| E::Limit)? > credit.max_work() {
        return Err(E::Limit);
    }
    let prepared =
        prepare_catalog_ack_request(body, limits.normal.codec, available).map_err(E::Codec)?;
    drop(prepared);
    Ok(EntryStamp::Stamped(stamp))
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    entry: &RaftLogEntry,
    fields: [&[u8]; 3],
    limits: &CommittedRecoveryLimits,
    base: usize,
) -> Result<ControlState, E> {
    selected(limits)?;
    let original = total(&[512, entry.body().len()])?;
    let scope_owner = total(&[512, fields[0].len(), fields[1].len(), fields[2].len()])?;
    let conversion = total(&[
        std::mem::size_of::<Vec<u8>>(),
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
    let outcome =
        apply_catalog_ack_transaction(state, &scope, entry.body(), entry.id(), limits.normal)
            .map_err(E::Credit)?;
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
