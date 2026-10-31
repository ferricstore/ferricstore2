//! Full historical audit and real atomic replay behind an explicit credit opt-in.

use super::{CommittedRecoveryError as E, CommittedRecoveryLimits};
use super::{replay, total};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;
use wf_domain::workflow::{
    control::{ControlState, EntryStamp},
    request::RequestScope,
};
use wf_ports::log::{RaftLogEntry, prepare_credit_request, prepare_public_credit_request};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, RedbStateTransaction};
use wf_storage::transaction::{
    CreditTransactionError, RecordBatchTransactionOutcome, apply_credit_transaction,
    apply_public_credit_transaction,
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::CommittedTime,
};

#[derive(Clone, Copy)]
pub(super) enum CreditRecoveryProfile {
    Identified(u64, CreditLimits),
    Catalog(u64, CreditLimits),
    Public(u64, CreditLimits),
}

impl CreditRecoveryProfile {
    const fn fields(self) -> (u16, u64, CreditLimits) {
        match self {
            Self::Identified(shards, credit) => (1, shards, credit),
            Self::Catalog(shards, credit) | Self::Public(shards, credit) => (2, shards, credit),
        }
    }
}

pub(super) const fn public(limits: &CommittedRecoveryLimits) -> bool {
    matches!(limits.credit, Some(CreditRecoveryProfile::Public(..)))
}

impl CommittedRecoveryLimits {
    /// Selects public credit callers plus existing catalog-capable private frames.
    /// # Errors
    /// Refuses invalid count, limits or unsupported recovery selection.
    pub fn with_public_credit_limits(self, shards: u64, credit: CreditLimits) -> Result<Self, E> {
        let mut selected = self.with_identified_credit_catalog_limits(shards, credit)?;
        selected.credit = Some(CreditRecoveryProfile::Public(shards, credit));
        Ok(selected)
    }

    /// Selects actual credit-format2 catalog replay and its immutable shard count.
    /// # Errors
    /// Refuses unsupported profile, invalid count or whole-call work overflow.
    pub fn with_identified_credit_catalog_limits(
        self,
        shard_count: u64,
        credit: CreditLimits,
    ) -> Result<Self, E> {
        let mut selected = self.with_identified_credit_limits(shard_count, credit)?;
        selected.credit = Some(CreditRecoveryProfile::Catalog(shard_count, credit));
        Ok(selected)
    }

    /// Selects credit replay with the actual configured immutable shard count.
    /// # Errors
    /// Refuses invalid shard counts or a whole-call work ceiling overflow.
    pub fn with_identified_credit_limits(
        mut self,
        shard_count: u64,
        credit: CreditLimits,
    ) -> Result<Self, E> {
        if !(1..=MAX).contains(&shard_count) {
            return Err(E::InvalidLimits);
        }
        u64::try_from(self.work.0)
            .ok()
            .and_then(|n| n.checked_mul(credit.max_work()))
            .ok_or(E::Limit)?;
        self.credit = Some(CreditRecoveryProfile::Identified(shard_count, credit));
        Ok(self)
    }

    /// Reads only the explicitly selected immutable credit profile.
    #[must_use]
    pub const fn identified_credit_limits(self) -> Option<(u64, CreditLimits)> {
        match self.credit {
            Some(profile) => {
                let (_, shards, credit) = profile.fields();
                Some((shards, credit))
            }
            None => None,
        }
    }
}

pub(super) fn require(
    tx: &RedbStateTransaction<'_>,
    limits: &CommittedRecoveryLimits,
) -> Result<(), E> {
    let Some(selected) = limits.credit else {
        return Ok(());
    };
    let (format, expected, _) = selected.fields();
    let actual = tx
        .require_identified_credit_profile()
        .map_err(|error| E::Credit(CreditTransactionError::State(error)))?;
    if actual != (format, expected) {
        return Err(E::Credit(CreditTransactionError::State(
            StateTransactionError::InvalidChanges,
        )));
    }
    Ok(())
}

pub(super) fn audit(
    body: &[u8],
    limits: &CommittedRecoveryLimits,
    base: usize,
    stamp: CommittedTime,
) -> Result<EntryStamp, E> {
    let (_, shards, credit) = limits.credit.ok_or(E::UnsupportedEntry)?.fields();
    let entry = total(&[512, body.len()])?;
    limits.admit(&[base, entry])?;
    let available = limits
        .workspace
        .checked_sub(total(&[base, entry, limits.additional_profile_bytes()?])?)
        .ok_or(E::Limit)?;
    let cost = body
        .len()
        .checked_mul(32)
        .and_then(|n| n.checked_add(65_536))
        .and_then(|n| {
            limits
                .normal
                .codec
                .admission()
                .batch_items()
                .min(1000)
                .checked_mul(16_384)
                .and_then(|ids| n.checked_add(ids))
        })
        .ok_or(E::Limit)?;
    if u64::try_from(cost).map_err(|_| E::Limit)? > credit.max_work() {
        return Err(E::Limit);
    }
    let prepared = if body.get(6) == Some(&15) && public(limits) {
        prepare_public_credit_request(body, limits.normal.codec, available)
    } else {
        prepare_credit_request(body, limits.normal.codec, available)
    }
    .map_err(E::Codec)?;
    if prepared.command().shard_count != shards {
        return Err(E::Credit(CreditTransactionError::State(
            StateTransactionError::InvalidChanges,
        )));
    }
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
    let (_, _, credit) = limits.credit.ok_or(E::UnsupportedEntry)?.fields();
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
    let outcome = if entry.body().get(6) == Some(&15) && public(limits) {
        apply_public_credit_transaction(
            state,
            &scope,
            entry.body(),
            entry.id(),
            limits.normal,
            credit,
        )
    } else {
        apply_credit_transaction(
            state,
            &scope,
            entry.body(),
            entry.id(),
            limits.normal,
            credit,
        )
    }
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
