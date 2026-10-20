//! Paid borrowed reached facts; only canonical predecessors survive reduction.

use super::super::load::{debit, entry, reserve_slots, sum};
use super::load::{LoadedClaim, copy};
use super::{ClaimTransactionError, ClaimTransactionLimits};
use std::collections::BTreeMap;
use wf_domain::claim::apply::{ClaimEncoding, ClaimObservedWindow};
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::policy::definition::{PolicySnapshot, StateMode};
use wf_domain::workflow::indexed::TouchedIndexedGroupView;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{ClaimApplyPlan, FreshClaimPhase, decide_fresh_claim_phase_observed};
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

pub(super) fn decide<'current>(
    fresh: FreshClaimPhase<'_>,
    scope: &RequestScope,
    loaded: &'current mut LoadedClaim,
    limits: ClaimTransactionLimits,
    live: usize,
) -> Result<ClaimApplyPlan<'current>, ClaimTransactionError> {
    let log = fresh
        .control()
        .next()
        .last_entry()
        .ok_or(StateTransactionError::Corrupt)?
        .log();
    let envelope = fresh
        .decoded()
        .claim_envelope(log)
        .ok_or(StateTransactionError::Corrupt)?;
    let (selected, scans) = (envelope.query.limit, envelope.scan_budget);
    let cap = limits.base.maximum_staging_bytes;
    debit(
        &mut loaded.staging_used,
        envelope.query.workflow_type.len(),
        cap,
    )?;
    let kind = copy(envelope.query.workflow_type)?;
    let LoadedClaim {
        queries,
        records,
        placements,
        projection,
        parallel,
        heads,
        state_keys,
        parallel_end,
        heads_complete,
        staging_used,
        ..
    } = loaded;
    let current = current_facts(queries, records, staging_used, cap)?;
    let encodings = encodings(queries, state_keys, staging_used, cap)?;
    let policy = projection.as_ref().map(|(policy, _)| policy.policy());
    let parallel_facts = parallel_facts(records, policy, staging_used, cap)?;
    let members = member_facts(records, policy, staging_used, cap)?;
    let fifo_heads = head_facts(&kind, heads, records, policy, staging_used, cap)?;
    let mut modes = BTreeMap::new();
    let mut retries = BTreeMap::new();
    debit(staging_used, 512, cap)?;
    modes.insert(kind.as_slice(), policy);
    retries.insert(kind.as_slice(), projection.as_ref().map(|(_, retry)| retry));
    let mut entries = Vec::new();
    reserve_slots(&mut entries, parallel.len(), staging_used, cap)?;
    for index in parallel.iter() {
        entries.push(row(records, *index)?);
    }
    let mut lanes = Vec::new();
    reserve_slots(&mut lanes, heads.len(), staging_used, cap)?;
    for (state, partition, _) in heads.iter() {
        lanes.push(LaneKey {
            workflow_type: &kind,
            state,
            partition_key: partition.as_bytes(),
        });
    }
    if *heads_complete {
        pure_slots(
            staging_used,
            (heads.len(), selected, scans, parallel.len()),
            cap,
        )?;
    }
    let window = ClaimObservedWindow {
        indexed: TouchedIndexedGroupView {
            group: scope.group(),
            placements,
            current: &current,
            parallel: &parallel_facts,
            fifo_members: &members,
            fifo_heads: &fifo_heads,
            policies: &modes,
            retry_policies: &retries,
        },
        encodings: &encodings,
        parallel: &entries,
        parallel_end: *parallel_end,
        head_lanes: &lanes,
        heads_complete: *heads_complete,
    };
    Ok(decide_fresh_claim_phase_observed(
        fresh,
        Some(window),
        live,
        limits.base.outcome_ttl_ms,
    )?)
}

fn current_facts<'query, 'current>(
    queries: &'query [(PartitionId, RecordId)],
    records: &'current [StoredWorkflowRecord],
    used: &mut usize,
    cap: usize,
) -> Result<BTreeMap<ScopedRecordId<'query>, Option<ClaimRecord<'current>>>, ClaimTransactionError>
{
    if queries.len() != records.len() {
        return Err(StateTransactionError::Corrupt.into());
    }
    let mut facts = BTreeMap::new();
    for ((partition, id), record) in queries.iter().zip(records) {
        if record.partition != *partition || record.record.id() != id {
            return Err(StateTransactionError::Corrupt.into());
        }
        debit(used, 256, cap)?;
        facts.insert(
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: id.as_bytes(),
            },
            Some(entry(record)),
        );
    }
    Ok(facts)
}

fn encodings<'query>(
    queries: &'query [(PartitionId, RecordId)],
    state_keys: &'query [Vec<u8>],
    used: &mut usize,
    cap: usize,
) -> Result<BTreeMap<ScopedRecordId<'query>, ClaimEncoding<'query>>, ClaimTransactionError> {
    if queries.len() != state_keys.len() {
        return Err(StateTransactionError::Corrupt.into());
    }
    let mut facts = BTreeMap::new();
    for ((partition, id), state_key) in queries.iter().zip(state_keys) {
        debit(used, 256, cap)?;
        facts.insert(
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: id.as_bytes(),
            },
            ClaimEncoding {
                partition,
                state_key,
            },
        );
    }
    Ok(facts)
}

fn parallel_facts<'current>(
    records: &'current [StoredWorkflowRecord],
    policy: Option<&PolicySnapshot>,
    used: &mut usize,
    cap: usize,
) -> Result<BTreeMap<DueOrder<'current>, bool>, ClaimTransactionError> {
    let mut facts = BTreeMap::new();
    for owner in records {
        let record = entry(owner);
        if let Some(due) = DueOrder::for_record(&record) {
            debit(used, 256, cap)?;
            facts.insert(due, mode(policy, record) == StateMode::Parallel);
        }
    }
    Ok(facts)
}

fn member_facts<'current>(
    records: &'current [StoredWorkflowRecord],
    policy: Option<&PolicySnapshot>,
    used: &mut usize,
    cap: usize,
) -> Result<
    BTreeMap<(LaneKey<'current>, LaneOrder<'current>), Option<ClaimRecord<'current>>>,
    ClaimTransactionError,
> {
    let mut facts = BTreeMap::new();
    for owner in records {
        let record = entry(owner);
        if mode(policy, record) == StateMode::Fifo {
            debit(used, 256, cap)?;
            facts.insert(
                (
                    LaneKey::for_record(&record).ok_or(StateTransactionError::Corrupt)?,
                    LaneOrder::for_record(&record),
                ),
                Some(record),
            );
        }
    }
    Ok(facts)
}

fn head_facts<'query, 'current>(
    kind: &'query [u8],
    heads: &'query [(Vec<u8>, PartitionId, Option<usize>)],
    records: &'current [StoredWorkflowRecord],
    policy: Option<&PolicySnapshot>,
    used: &mut usize,
    cap: usize,
) -> Result<
    BTreeMap<LaneKey<'query>, Option<(LaneOrder<'current>, ClaimRecord<'current>)>>,
    ClaimTransactionError,
>
where
    'current: 'query,
{
    let mut facts = BTreeMap::new();
    for (state, partition, index) in heads {
        let lane = LaneKey {
            workflow_type: kind,
            state,
            partition_key: partition.as_bytes(),
        };
        let value = index
            .map(|index| row(records, index).map(|row| (LaneOrder::for_record(&row), row)))
            .transpose()?;
        debit(used, 256, cap)?;
        facts.insert(lane, value);
    }
    for owner in records {
        let record = entry(owner);
        let lane = LaneKey::for_record(&record).ok_or(StateTransactionError::Corrupt)?;
        let value = (mode(policy, record) == StateMode::Fifo)
            .then_some((LaneOrder::for_record(&record), record));
        if let Some(previous) = facts.get(&lane) {
            if previous != &value {
                return Err(StateTransactionError::Corrupt.into());
            }
        } else {
            debit(used, 256, cap)?;
            facts.insert(lane, value);
        }
    }
    Ok(facts)
}

fn mode(policy: Option<&PolicySnapshot>, row: ClaimRecord<'_>) -> StateMode {
    policy.map_or(StateMode::Parallel, |policy| {
        policy.state_mode(row.logical_state())
    })
}

fn row(
    records: &[StoredWorkflowRecord],
    index: usize,
) -> Result<ClaimRecord<'_>, ClaimTransactionError> {
    records
        .get(index)
        .map(entry)
        .ok_or_else(|| StateTransactionError::Corrupt.into())
}

fn pure_slots(
    used: &mut usize,
    (heads, selected, scans, parallel): (usize, usize, usize, usize),
    cap: usize,
) -> Result<(), ClaimTransactionError> {
    // Existing pure heads + six per-selected vector spines, and every set slot.
    let vectors = selected
        .checked_mul(6)
        .and_then(|count| count.checked_add(heads))
        .and_then(|count| count.checked_mul(512))
        .ok_or_else(super::super::load::overflow)?;
    let inspected = sum(&[parallel, heads])?.min(scans);
    let sets = heads
        .checked_mul(2)
        .and_then(|count| count.checked_add(inspected))
        .and_then(|count| count.checked_add(selected))
        .and_then(|count| count.checked_mul(256))
        .ok_or_else(super::super::load::overflow)?;
    debit(used, sum(&[vectors, sets])?, cap)?;
    Ok(())
}
