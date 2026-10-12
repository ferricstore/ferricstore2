//! Identified credit value checks and refund planning, never spend authority.

use super::admission::{ClaimCoordinationError, ClaimCoordinationLimits, charge_coordination_work};
use super::{RunningCreditDemand, RunningCreditDemandKind};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::active::LimitEnforcement;
use std::collections::BTreeSet;

/// Private immutable value uniqueness, independent of external spend/expiry.
pub struct ClaimCreditBatch<'a> {
    scope: &'a [u8],
    shard_id: u64,
    epoch: u64,
    enforcement: LimitEnforcement,
    reservation_ids: &'a [&'a [u8]],
}
impl<'a> ClaimCreditBatch<'a> {
    /// Original admitted scope owner.
    #[must_use]
    pub const fn scope(&self) -> &'a [u8] {
        self.scope
    }
    /// Captured shard observation, without routing/spend proof.
    #[must_use]
    pub const fn shard_id(&self) -> u64 {
        self.shard_id
    }
    /// Captured epoch observation, without owner freshness proof.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }
    /// Captured demand enforcement.
    #[must_use]
    pub const fn enforcement(&self) -> LimitEnforcement {
        self.enforcement
    }
    /// Original reservation spine, with a short object-bound slice borrow.
    #[must_use]
    pub const fn reservation_ids(&self) -> &[&'a [u8]] {
        self.reservation_ids
    }
}
/// Untrusted external-result classification; no constructor confirms a spend.
#[derive(Clone, Copy)]
pub enum ClaimSpendOutcome<'a> {
    /// Reported confirmation requires authoritative verification by its owner.
    Confirmed(&'a ClaimCreditBatch<'a>),
    /// Known spend failure, without any manufactured empty-claim proof.
    KnownFailure,
    /// Unresolved result retains the exact original IDs for owner inquiry.
    Unknown(&'a ClaimCreditBatch<'a>),
}
/// Pure classification, never permission to skip the external owner protocol.
#[derive(Clone, Copy)]
pub enum ClaimCreditUse<'a> {
    /// Reported success retains the same batch.
    Proceed(&'a ClaimCreditBatch<'a>),
    /// Known failure cannot proceed as an empty claim.
    Reject,
    /// Resolve the original batch before any new spend/claim.
    ResolveUnknown(&'a ClaimCreditBatch<'a>),
}
/// Reported atomic execution outcome, not established by this value model.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ClaimExecutionOutcome<'a> {
    /// Actual used identities, including repeated/non-prefix observations.
    Accepted(&'a [&'a [u8]]),
    /// Externally proven atomic no-commit, never an arbitrary Router error.
    KnownNoCommit,
    /// Commit is unresolved: release none.
    UnknownCommit,
    /// Unclassified result: release none.
    Unclassified,
}
/// Refund meaning, without creating an outbox or owner transaction.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum CreditSettlementDisposition {
    /// Refund only identities absent from the actual used set.
    RefundUnused,
    /// Refund every identity after externally established atomic no-commit.
    RefundNoCommit,
    /// Hold exact original identities while commit is unresolved.
    HoldUnknown,
    /// Hold exact original identities until classification is authoritative.
    HoldUnclassified,
}
/// Owns only refund references, retaining the original batch/leaf owners.
pub struct ClaimCreditSettlement<'a> {
    batch: &'a ClaimCreditBatch<'a>,
    disposition: CreditSettlementDisposition,
    refund_ids: Vec<&'a [u8]>,
}
impl<'a> ClaimCreditSettlement<'a> {
    /// Original immutable batch for settlement or inquiry.
    #[must_use]
    pub const fn batch(&self) -> &'a ClaimCreditBatch<'a> {
        self.batch
    }
    /// Pure refund/hold disposition.
    #[must_use]
    pub const fn disposition(&self) -> CreditSettlementDisposition {
        self.disposition
    }
    /// Refund spine borrows this object; leaves retain their original owners.
    #[must_use]
    pub fn refund_ids(&self) -> &[&'a [u8]] {
        &self.refund_ids
    }
}

/// Admits one identified batch after complete work/descriptor preflight.
///
/// Only this constructor builds a reserved-ID duplicate set. Its safe immutable
/// borrows prove value uniqueness; they do not prove an external owner's spend.
///
/// # Errors
/// Rejects count/identity/scalar/scope mismatches or current work profile.
pub fn admit_claim_credit_batch<'a>(
    demand: &RunningCreditDemand<'_, '_>,
    scope: &'a [u8],
    shard_id: u64,
    epoch: u64,
    ids: &'a [&'a [u8]],
    limits: ClaimCoordinationLimits,
) -> Result<ClaimCreditBatch<'a>, ClaimCoordinationError> {
    if ids.is_empty()
        || ids.len() > 1000
        || u64::try_from(ids.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?
            != demand.amount
    {
        return Err(ClaimCoordinationError::InvalidCreditAuthority);
    }
    let mut work = 0;
    charge_coordination_work(
        &mut work,
        u64::try_from(ids.len()).map_err(|_| ClaimCoordinationError::WorkBudget)? * 64,
        limits,
    )?;
    admit_credit_occurrences(scope, shard_id, epoch, ids, &[], limits, &mut work)?;
    if let RunningCreditDemandKind::Explicit {
        scope: expected,
        shard_id: expected_shard,
    } = demand.kind
    {
        if expected.is_empty() || expected.len() > 65_535 {
            return Err(ClaimCoordinationError::InvalidCreditAuthority);
        }
        let bytes = scope
            .len()
            .checked_add(expected.len())
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        charge_coordination_work(
            &mut work,
            u64::try_from(bytes).map_err(|_| ClaimCoordinationError::WorkBudget)?,
            limits,
        )?;
        if scope != expected || shard_id != expected_shard {
            return Err(ClaimCoordinationError::InvalidCreditAuthority);
        }
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(*id) {
            return Err(ClaimCoordinationError::InvalidCreditAuthority);
        }
    }
    Ok(ClaimCreditBatch {
        scope,
        shard_id,
        epoch,
        enforcement: demand.enforcement,
        reservation_ids: ids,
    })
}

/// Retains exact batch references; all remote ownership remains unverified.
#[must_use]
pub const fn classify_claim_spend(outcome: ClaimSpendOutcome<'_>) -> ClaimCreditUse<'_> {
    match outcome {
        ClaimSpendOutcome::Confirmed(batch) => ClaimCreditUse::Proceed(batch),
        ClaimSpendOutcome::KnownFailure => ClaimCreditUse::Reject,
        ClaimSpendOutcome::Unknown(batch) => ClaimCreditUse::ResolveUnknown(batch),
    }
}

/// Re-admits current occurrences before set/refund ownership; never re-proves
/// reserved uniqueness with a second set. Unknown/unclassified release none.
///
/// # Errors
/// Rejects invalid observations/current work or fallible refund-spine growth.
pub fn plan_claim_credit_settlement<'a>(
    batch: &'a ClaimCreditBatch<'a>,
    outcome: ClaimExecutionOutcome<'_>,
    limits: ClaimCoordinationLimits,
) -> Result<ClaimCreditSettlement<'a>, ClaimCoordinationError> {
    let used = match outcome {
        ClaimExecutionOutcome::Accepted(ids) => ids,
        _ => &[],
    };
    if batch.reservation_ids.len() > 1000 || used.len() > 1000 {
        return Err(ClaimCoordinationError::InvalidCreditAuthority);
    }
    let reserved = u64::try_from(batch.reservation_ids.len())
        .map_err(|_| ClaimCoordinationError::WorkBudget)?;
    let mut work = 0;
    let descriptors = match outcome {
        ClaimExecutionOutcome::Accepted(_) => reserved.checked_mul(16).and_then(|n| {
            u64::try_from(used.len())
                .ok()?
                .checked_mul(64)?
                .checked_add(n)
        }),
        ClaimExecutionOutcome::KnownNoCommit => reserved.checked_mul(16),
        ClaimExecutionOutcome::UnknownCommit | ClaimExecutionOutcome::Unclassified => Some(0),
    }
    .ok_or(ClaimCoordinationError::WorkBudget)?;
    charge_coordination_work(&mut work, descriptors, limits)?;
    admit_credit_occurrences(
        batch.scope,
        batch.shard_id,
        batch.epoch,
        batch.reservation_ids,
        used,
        limits,
        &mut work,
    )?;
    let (disposition, refund_ids) = match outcome {
        ClaimExecutionOutcome::Accepted(_) => (
            CreditSettlementDisposition::RefundUnused,
            collect_refund_ids(batch.reservation_ids, used)?,
        ),
        ClaimExecutionOutcome::KnownNoCommit => {
            let mut refunds = Vec::new();
            refunds
                .try_reserve_exact(batch.reservation_ids.len())
                .map_err(|_| ClaimCoordinationError::AllocationFailure)?;
            for id in batch.reservation_ids {
                refunds.push(*id);
            }
            (CreditSettlementDisposition::RefundNoCommit, refunds)
        }
        ClaimExecutionOutcome::UnknownCommit => {
            (CreditSettlementDisposition::HoldUnknown, Vec::new())
        }
        ClaimExecutionOutcome::Unclassified => {
            (CreditSettlementDisposition::HoldUnclassified, Vec::new())
        }
    };
    Ok(ClaimCreditSettlement {
        batch,
        disposition,
        refund_ids,
    })
}

fn admit_credit_occurrences(
    scope: &[u8],
    shard: u64,
    epoch: u64,
    reserved: &[&[u8]],
    used: &[&[u8]],
    limits: ClaimCoordinationLimits,
    work: &mut u64,
) -> Result<(), ClaimCoordinationError> {
    if reserved.is_empty()
        || reserved.len() > 1000
        || used.len() > 1000
        || shard > MAX_EXACT_MILLIS
        || epoch > MAX_EXACT_MILLIS
    {
        return Err(ClaimCoordinationError::InvalidCreditAuthority);
    }
    let occurrences = reserved
        .len()
        .checked_add(used.len())
        .ok_or(ClaimCoordinationError::WorkBudget)?;
    charge_coordination_work(
        work,
        u64::try_from(occurrences).map_err(|_| ClaimCoordinationError::WorkBudget)? * 64,
        limits,
    )?;
    if scope.is_empty() || scope.len() > 65_535 {
        return Err(ClaimCoordinationError::InvalidCreditAuthority);
    }
    charge_coordination_work(
        work,
        u64::try_from(scope.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    for id in reserved.iter().chain(used) {
        if id.is_empty() || id.len() > 256 {
            return Err(ClaimCoordinationError::InvalidCreditAuthority);
        }
        charge_coordination_work(
            work,
            u64::try_from(id.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?,
            limits,
        )?;
    }
    Ok(())
}
fn collect_refund_ids<'a>(
    reserved: &'a [&'a [u8]],
    used: &[&[u8]],
) -> Result<Vec<&'a [u8]>, ClaimCoordinationError> {
    let mut used_ids = BTreeSet::new();
    for id in used {
        used_ids.insert(*id);
    }
    let mut refunds = Vec::new();
    for id in reserved {
        if !used_ids.contains(*id) {
            refunds
                .try_reserve(1)
                .map_err(|_| ClaimCoordinationError::AllocationFailure)?;
            refunds.push(*id);
        }
    }
    Ok(refunds)
}
