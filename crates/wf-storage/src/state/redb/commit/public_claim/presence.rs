//! Only complete physical due exhaustion can prove source `NoDemand`.
use super::{Limits, codec};
use crate::state::redb::store::RedbStateTransaction;
use wf_domain::claim::{
    PartitionFilter,
    apply::{ClaimQuery, ClaimStates},
    caller::ClaimPartitionMode,
};
use wf_ports::{
    log::FreshAtomicClaim,
    state::{
        ClaimDueStep, GroupApplyTransaction, GroupClaimDueCursor, GroupClaimTransaction,
        StateTransactionError as E,
    },
};

pub(super) fn definitely_empty(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    limits: Limits,
) -> Result<bool, E> {
    let call = fresh.prepared().caller(limits.base.caller).map_err(codec)?;
    let (any, names) = call.states();
    let (mode, keys) = call.partitions();
    let partitions = match mode {
        ClaimPartitionMode::Any => PartitionFilter::Any,
        ClaimPartitionMode::Only => PartitionFilter::Only(keys),
        ClaimPartitionMode::Auto | ClaimPartitionMode::Global => {
            return Err(E::UnsupportedPolicyProfile);
        }
    };
    let priority = call
        .priority()
        .map(u8::try_from)
        .transpose()
        .map_err(|_| E::InvalidChanges)?;
    if priority.is_some_and(|n| n > 2) {
        return Err(E::InvalidChanges);
    }
    let query = ClaimQuery {
        workflow_type: call.workflow_type(),
        partitions,
        states: if any {
            ClaimStates::Any
        } else {
            ClaimStates::Only(names)
        },
        exclude_states: &[b"running"],
        priority,
        worker: call.worker(),
        lease_ms: call.lease_ms(),
        limit: usize::try_from(call.limit()).map_err(|_| E::Limit)?,
    };
    // Pinned API normalizes scalar and deduplicated singleton lists alike
    // (claim_due_api.ex:1189-1195,1239-1243).
    if !any && names == [b"running".as_slice()] {
        return empty(
            tx,
            fresh,
            ClaimQuery {
                states: ClaimStates::Only(&[b"running"]),
                exclude_states: &[],
                ..query
            },
        );
    }
    if call.reclaim_expired() && call.reclaim_ratio() == 0 {
        return Ok(false);
    }
    if !empty(tx, fresh, query)? {
        return Ok(false);
    }
    if !call.reclaim_expired() {
        return Ok(true);
    }
    empty(
        tx,
        fresh,
        ClaimQuery {
            states: ClaimStates::Only(&[b"running"]),
            exclude_states: &[],
            ..query
        },
    )
}

fn empty(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    query: ClaimQuery<'_>,
) -> Result<bool, E> {
    let mut due = tx.claim_due_cursor(
        fresh.prepared().scope().tenant(),
        query,
        fresh.control().execution_time(),
    )?;
    loop {
        match due.next()? {
            ClaimDueStep::Row { due, .. } => {
                tx.temporary(
                    due.partition_key
                        .len()
                        .checked_add(due.id.len())
                        .and_then(|n| n.checked_add(512))
                        .ok_or(E::Limit)?,
                )?;
                let partition =
                    wf_types::identity::PartitionId::new(super::funding::copy(due.partition_key)?)
                        .map_err(|_| E::Corrupt)?;
                let id = wf_types::identity::RecordId::new(super::funding::copy(due.id)?)
                    .map_err(|_| E::Corrupt)?;
                let stored = tx
                    .record(fresh.prepared().scope().tenant(), &partition, &id)?
                    .ok_or(E::Corrupt)?;
                let actual = wf_domain::claim::ClaimRecord {
                    record: &stored.record,
                    partition_key: stored.partition.as_bytes(),
                    priority: stored.ordering.priority,
                    state_enter_seq: stored.ordering.state_enter_seq,
                };
                if wf_domain::claim::DueOrder::for_record(&actual) != Some(due) {
                    return Err(E::Corrupt);
                }
                let state = stored.record.state();
                let included = match query.states {
                    ClaimStates::Any => true,
                    ClaimStates::Only(names) => names.contains(&state),
                };
                if included && !query.exclude_states.contains(&state) {
                    return Ok(false);
                }
            }
            ClaimDueStep::RangeEnd => return Ok(true),
            ClaimDueStep::BudgetEnd => return Err(E::Limit),
        }
    }
}
