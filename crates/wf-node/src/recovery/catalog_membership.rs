//! Complete membership historical audit and replay only under explicit selection.

use super::{CommittedRecoveryError as E, CommittedRecoveryLimits, replay, total};
use wf_domain::workflow::{
    control::{ControlState, EntryStamp},
    request::RequestScope,
};
use wf_ports::log::{RaftLogEntry, prepare_catalog_membership_request};
use wf_storage::{
    state::RedbStateStore,
    transaction::{RecordBatchTransactionOutcome, apply_catalog_membership_transaction},
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::CommittedTime,
};

#[derive(Clone, Copy)]
pub(super) enum GovernanceCatalogRecoveryProfile {
    Disabled,
    Enabled,
    Progress,
    Prune,
}
impl CommittedRecoveryLimits {
    /// Explicitly selects GC3 registration revisions and conditional pruning.
    #[must_use]
    pub const fn with_governance_catalog_pruning(mut self) -> Self {
        self.governance_catalog = GovernanceCatalogRecoveryProfile::Prune;
        self
    }
    /// Explicitly enables target catalog bodies on the normal apply profile.
    #[must_use]
    pub const fn with_governance_catalogs(mut self) -> Self {
        self.governance_catalog = GovernanceCatalogRecoveryProfile::Enabled;
        self
    }
    /// Explicitly enables GC2 membership and durable cleaner progress bodies.
    #[must_use]
    pub const fn with_governance_catalog_progress(mut self) -> Self {
        self.governance_catalog = GovernanceCatalogRecoveryProfile::Progress;
        self
    }
}
pub(super) const fn enabled(limits: &CommittedRecoveryLimits) -> bool {
    matches!(
        limits.governance_catalog,
        GovernanceCatalogRecoveryProfile::Enabled
            | GovernanceCatalogRecoveryProfile::Progress
            | GovernanceCatalogRecoveryProfile::Prune
    )
}
pub(super) fn require(
    tx: &wf_storage::state::RedbStateTransaction<'_>,
    limits: &CommittedRecoveryLimits,
    actual: u16,
) -> Result<(), E> {
    require_profile(limits, actual)?;
    match limits.governance_catalog {
        GovernanceCatalogRecoveryProfile::Disabled => Ok(()),
        GovernanceCatalogRecoveryProfile::Enabled => {
            tx.require_governance_catalog().map_err(E::State)
        }
        GovernanceCatalogRecoveryProfile::Progress => {
            tx.require_catalog_progress().map_err(E::State)
        }
        GovernanceCatalogRecoveryProfile::Prune => Err(E::State(
            wf_ports::state::StateTransactionError::UnsupportedPolicyProfile,
        )),
    }
}
pub(super) fn require_profile(limits: &CommittedRecoveryLimits, actual: u16) -> Result<(), E> {
    match (limits.governance_catalog, actual) {
        (GovernanceCatalogRecoveryProfile::Disabled, 0 | 1)
        | (GovernanceCatalogRecoveryProfile::Enabled, 1)
        | (GovernanceCatalogRecoveryProfile::Progress, 2)
        | (GovernanceCatalogRecoveryProfile::Prune, 3) => Ok(()),
        _ => Err(E::State(
            wf_ports::state::StateTransactionError::UnsupportedPolicyProfile,
        )),
    }
}
pub(super) fn audit(
    body: &[u8],
    limits: &CommittedRecoveryLimits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, E> {
    if !enabled(limits) {
        return Err(E::UnsupportedEntry);
    }
    let entry = total(&[512, body.len()])?;
    limits.admit(&[base, entry])?;
    let reserved = total(&[
        limits.normal.maximum_read_bytes,
        limits.normal.maximum_staging_bytes,
        limits.normal.maximum_adapter_bytes,
    ])?;
    let normal = limits
        .normal
        .workspace_bytes
        .checked_sub(reserved)
        .ok_or(E::Limit)?;
    let available = limits
        .workspace
        .checked_sub(total(&[base, entry, limits.additional_profile_bytes()?])?)
        .ok_or(E::Limit)?
        .min(normal);
    let prepared = prepare_catalog_membership_request(body, limits.normal.codec, available)
        .map_err(E::Codec)?;
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
    if !enabled(limits) {
        return Err(E::UnsupportedEntry);
    }
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
    let outcome = apply_catalog_membership_transaction(
        state,
        &scope,
        entry.body(),
        entry.id(),
        limits.normal,
    )
    .map_err(E::CatalogMembership)?;
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
